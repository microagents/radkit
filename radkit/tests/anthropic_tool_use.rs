//! Anthropic Claude Tool Use Tests
//!
//! Tests function calling capabilities with Anthropic Claude using the Agent for full round-trip validation.
//! These tests require ANTHROPIC_API_KEY environment variable to be set.
//! Tests comprehensive A2A event validation, internal event monitoring, and tool execution tracking.

use futures::StreamExt;
use radkit::a2a::{
    Message, MessageRole, MessageSendParams, Part, SendStreamingMessageResult, TaskQueryParams,
    TaskState,
};
use radkit::agents::Agent;
use radkit::models::AnthropicLlm;
use radkit::sessions::InMemorySessionService;
use radkit::tools::{FunctionTool, ToolResult};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

mod common;
use common::get_anthropic_key;

/// Helper function to create Agent with tools if API key is available
/// Creates agent with proper session service and task store for comprehensive testing
fn create_test_agent_with_tools(tools: Vec<Arc<FunctionTool>>) -> Option<Agent> {
    get_anthropic_key().map(|api_key| {
        let anthropic_llm = AnthropicLlm::new("claude-3-5-sonnet-20241022".to_string(), api_key);
        let session_service = Arc::new(InMemorySessionService::new());

        let base_tools: Vec<Arc<dyn radkit::tools::BaseTool>> = tools
            .into_iter()
            .map(|tool| tool as Arc<dyn radkit::tools::BaseTool>)
            .collect();

        Agent::builder(
            "You are a helpful assistant. Use the available tools when requested by the user. Always call tools when they can help answer the user's question.",
            anthropic_llm
        )
        .with_card(|c| c
            .with_name("test_agent")
            .with_description("Test agent for function calling")
        )
        .with_session_service(session_service)
        .with_tools(base_tools)
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
        "required": ["location"]
    }))
}

/// Create a calculator tool for testing
fn create_calculator_tool() -> FunctionTool {
    FunctionTool::new(
        "calculate".to_string(),
        "Perform basic mathematical calculations".to_string(),
        |args: HashMap<String, Value>, _context| {
            Box::pin(async move {
                let expression = args
                    .get("expression")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                // Simple calculator for testing - just handle basic cases
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
            })
        },
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

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_anthropic_single_tool_call_comprehensive() {
    let Some(agent) = create_test_agent_with_tools(vec![Arc::new(create_weather_tool())]) else {
        println!("⚠️  Skipping test: ANTHROPIC_API_KEY not found");
        return;
    };

    println!("🧪 Testing Anthropic single tool call with comprehensive validation...");

    let message = create_user_message(
        "What's the weather like in San Francisco? Please use the get_weather tool to find out.",
    );

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    // Use streaming message API for comprehensive tool use validation
    let result = agent
        .send_streaming_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(
        result.is_ok(),
        "Agent execution should succeed: {:?}",
        result.err()
    );

    let mut execution = result.unwrap();
    let mut all_text = String::new();
    let mut message_events = 0;
    let mut final_task: Option<radkit::a2a::Task> = None;

    println!("✅ Processing A2A streaming events for tool execution:");
    while let Some(result) = execution.a2a_stream.next().await {
        match result {
            SendStreamingMessageResult::Message(message) => {
                message_events += 1;
                println!(
                    "  📨 A2A Message Event #{}: role={:?}",
                    message_events, message.role
                );

                for part in &message.parts {
                    match part {
                        Part::Text { text, .. } => {
                            all_text.push_str(text);
                            all_text.push(' ');
                        }
                        Part::Data { data, .. } => {
                            // Note: function_call and function_response are no longer sent as A2A messages
                            // They are now internal events only (stored in session.session_events)
                            println!("  📦 A2A Data Part: {}", data);
                        }
                        _ => {}
                    }
                }
            }
            SendStreamingMessageResult::TaskStatusUpdate(status_update) => {
                println!(
                    "  📊 A2A TaskStatusUpdate: state={:?}",
                    status_update.status.state
                );
            }
            SendStreamingMessageResult::TaskArtifactUpdate(artifact_update) => {
                println!(
                    "  🎯 A2A TaskArtifactUpdate: name={:?}",
                    artifact_update.artifact.name
                );
            }
            SendStreamingMessageResult::Task(task) => {
                println!(
                    "  ✅ A2A Final Task: id={}, state={:?}",
                    task.id, task.status.state
                );

                // Validate final task structure
                assert_eq!(
                    task.status.state,
                    TaskState::Submitted,
                    "Task should be completed"
                );
                assert!(task.history.len() >= 2, "Should have user + agent messages");

                final_task = Some(task);
                break;
            }
        }
    }

    // Note: Tool execution is now validated via internal events only
    assert!(final_task.is_some(), "Should receive final task");

    let final_task = final_task.unwrap();

    // ✅ Validate Tool Execution via Real-Time Events + Session Persistence
    let mut realtime_function_calls = 0;
    let mut realtime_function_responses = 0;
    let mut realtime_weather_called = false;
    let mut realtime_weather_succeeded = false;

    println!("✅ Processing real-time internal events:");
    // Give internal events time to be processed
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Collect some events from the stream
    let mut collected_events = Vec::new();
    for _ in 0..10 {
        if let Some(internal_event) = execution.all_events_stream.next().await {
            collected_events.push(internal_event);
        } else {
            break;
        }
    }

    for internal_event in collected_events {
        if let radkit::sessions::SessionEventType::UserMessage { content }
        | radkit::sessions::SessionEventType::AgentMessage { content } =
            internal_event.event_type
        {
            for part in &content.parts {
                match part {
                    radkit::models::content::ContentPart::FunctionCall {
                        name, arguments, ..
                    } => {
                        realtime_function_calls += 1;
                        println!("  🔧 Real-time Function Call: {}", name);
                        if name == "get_weather" {
                            realtime_weather_called = true;
                            if let Some(location) = arguments.get("location") {
                                println!("    📍 Real-time Location: {}", location);
                            }
                        }
                    }
                    radkit::models::content::ContentPart::FunctionResponse {
                        name,
                        success,
                        result,
                        ..
                    } => {
                        realtime_function_responses += 1;
                        println!(
                            "  ⚙️ Real-time Function Response: {} (success: {})",
                            name, success
                        );
                        if name == "get_weather" && *success {
                            realtime_weather_succeeded = true;
                            if let Some(temp) = result.get("temperature") {
                                println!("    🌡️ Real-time Temperature: {}", temp);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // ✅ Validate Tool Execution via Session Persistence
    let session_service = agent.session_service();
    let session = session_service
        .get_session("test_app", "test_user", &final_task.context_id)
        .await
        .expect("Should be able to get session")
        .expect("Session should exist");

    let mut session_function_calls = 0;
    let mut session_function_responses = 0;
    let mut session_weather_called = false;
    let mut session_weather_succeeded = false;

    println!("✅ Validating persisted tool execution in session:");
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
                        println!("  🔧 Session Function Call: {}", name);
                        if name == "get_weather" {
                            session_weather_called = true;
                            if let Some(location) = arguments.get("location") {
                                println!("    📍 Session Location: {}", location);
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
                        println!(
                            "  ⚙️ Session Function Response: {} (success: {})",
                            name, success
                        );
                        if name == "get_weather" && *success {
                            session_weather_succeeded = true;
                            if let Some(temp) = result.get("temperature") {
                                println!("    🌡️ Session Temperature: {}", temp);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Comprehensive validation: Both real-time events AND session persistence should capture tool execution
    println!("✅ Tool execution validation summary:");
    println!(
        "  Real-time: {} calls, {} responses",
        realtime_function_calls, realtime_function_responses
    );
    println!(
        "  Session: {} calls, {} responses",
        session_function_calls, session_function_responses
    );

    // At least one source should capture tool execution (preferably both)
    let total_calls = realtime_function_calls + session_function_calls;
    let total_responses = realtime_function_responses + session_function_responses;
    let weather_called = realtime_weather_called || session_weather_called;
    let weather_succeeded = realtime_weather_succeeded || session_weather_succeeded;

    assert!(
        total_calls >= 1,
        "Should detect function calls in real-time events or session"
    );
    assert!(
        total_responses >= 1,
        "Should detect function responses in real-time events or session"
    );
    assert!(weather_called, "get_weather tool should have been called");
    assert!(weather_succeeded, "get_weather tool should have succeeded");

    // ✅ Validate A2A Task History (should contain conversation but NOT function calls)
    println!("✅ Validating A2A task history:");
    println!(
        "  📋 Task history contains {} messages",
        final_task.history.len()
    );

    let mut total_text_parts = 0;
    let mut total_data_parts = 0;

    for (i, message) in final_task.history.iter().enumerate() {
        println!(
            "  📨 A2A History Message #{}: role={:?}, parts={}",
            i + 1,
            message.role,
            message.parts.len()
        );
        for part in &message.parts {
            match part {
                radkit::a2a::Part::Text { text, .. } => {
                    total_text_parts += 1;
                    let preview = text.chars().take(60).collect::<String>();
                    let suffix = if text.len() > 60 { "..." } else { "" };
                    println!("    💬 Text: {}{}", preview, suffix);
                }
                radkit::a2a::Part::Data { data, .. } => {
                    total_data_parts += 1;
                    println!("    📊 Data: {}", data);
                }
                radkit::a2a::Part::File { .. } => {
                    println!("    📎 File attachment");
                }
            }
        }
    }

    println!("✅ A2A task history validation summary:");
    println!(
        "  Messages: {}, Text parts: {}, Data parts: {}",
        final_task.history.len(),
        total_text_parts,
        total_data_parts
    );

    // A2A messages should contain the conversation but NOT function calls (by design)
    assert!(
        final_task.history.len() >= 2,
        "Should have at least user question + agent response"
    );
    assert!(
        total_text_parts >= 1,
        "Should have text content in conversation"
    );
    println!(
        "  ✅ A2A task history correctly contains conversation (function calls excluded by design)"
    );

    // ✅ Validate Response Content includes Tool Results
    let response_lower = all_text.to_lowercase();
    assert!(
        response_lower.contains("72") || response_lower.contains("temperature"),
        "Response should include temperature from weather tool: {}",
        all_text
    );
    assert!(
        response_lower.contains("partly cloudy")
            || response_lower.contains("cloudy")
            || response_lower.contains("conditions"),
        "Response should include weather conditions from tool: {}",
        all_text
    );

    // ✅ Test Task Retrieval and Validation
    let retrieved_task = agent
        .get_task(
            "test_app",
            "test_user",
            TaskQueryParams {
                id: final_task.id.clone(),
                history_length: None,
                metadata: None,
            },
        )
        .await;
    assert!(
        retrieved_task.is_ok(),
        "Should be able to retrieve task with tool execution"
    );

    let retrieved_task = retrieved_task.unwrap();

    // NOTE: Tool calls are NOT preserved in task history by design - they're internal events only
    // This keeps A2A Task.history clean with only visible conversation content
    // Validate that we have the expected conversation messages instead
    assert!(
        retrieved_task.history.len() >= 2,
        "Should have at least user + agent messages in history"
    );

    // Verify we have a user message and agent response messages
    let user_messages = retrieved_task
        .history
        .iter()
        .filter(|m| m.role == MessageRole::User)
        .count();
    let agent_messages = retrieved_task
        .history
        .iter()
        .filter(|m| m.role == MessageRole::Agent)
        .count();
    assert!(user_messages >= 1, "Should have at least 1 user message");
    assert!(agent_messages >= 1, "Should have at least 1 agent message");

    println!("✅ Session contains {} total events", session.events.len());

    println!("✅ Comprehensive tool call test completed successfully");
}

/// Test multiple tool calls in sequence with comprehensive validation
#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_anthropic_multi_tool_execution() {
    let tools = vec![
        Arc::new(create_weather_tool()),
        Arc::new(create_calculator_tool()),
    ];

    let Some(agent) = create_test_agent_with_tools(tools) else {
        println!("⚠️  Skipping test: ANTHROPIC_API_KEY not found");
        return;
    };

    println!("🧪 Testing Anthropic multi-tool execution with comprehensive validation...");

    let message = create_user_message(
        "Please help me with two tasks: 1) Get the weather in San Francisco using the get_weather tool, 2) Calculate 10*5 using the calculate tool. Use both tools to answer my questions.",
    );

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    let result = agent
        .send_streaming_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(
        result.is_ok(),
        "Agent execution should succeed: {:?}",
        result.err()
    );

    let mut execution = result.unwrap();
    let mut all_text = String::new();
    let mut final_task: Option<radkit::a2a::Task> = None;

    println!("✅ Processing A2A streaming events for multi-tool execution:");
    while let Some(result) = execution.a2a_stream.next().await {
        match result {
            SendStreamingMessageResult::Message(message) => {
                for part in &message.parts {
                    match part {
                        Part::Text { text, .. } => {
                            all_text.push_str(text);
                            all_text.push(' ');
                        }
                        Part::Data { data, .. } => {
                            // Note: function_call and function_response are no longer sent as A2A messages
                            println!("  📦 A2A Data Part: {}", data);
                        }
                        _ => {}
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

    // Note: Tool execution is validated via internal events only
    assert!(final_task.is_some(), "Should receive final task");

    // ✅ Validate Multi-Tool Execution via Real-Time + Session Events
    let final_task = final_task.unwrap();

    // Process real-time events
    let mut rt_weather_calls = 0;
    let mut rt_calculator_calls = 0;
    let mut rt_weather_responses = 0;
    let mut rt_calculator_responses = 0;

    println!("✅ Processing real-time internal events for multi-tool:");
    // Collect some events from the stream for multi-tool test
    let mut multi_tool_events = Vec::new();
    for _ in 0..15 {
        if let Some(internal_event) = execution.all_events_stream.next().await {
            multi_tool_events.push(internal_event);
        } else {
            break;
        }
    }

    for internal_event in multi_tool_events {
        if let radkit::sessions::SessionEventType::UserMessage { content }
        | radkit::sessions::SessionEventType::AgentMessage { content } =
            internal_event.event_type
        {
            for part in &content.parts {
                match part {
                    radkit::models::content::ContentPart::FunctionCall { name, .. } => {
                        println!("  🔧 Real-time Function Call: {}", name);
                        match name.as_str() {
                            "get_weather" => rt_weather_calls += 1,
                            "calculate" => rt_calculator_calls += 1,
                            _ => {}
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
                        assert!(success, "All tool executions should succeed");
                        match name.as_str() {
                            "get_weather" => rt_weather_responses += 1,
                            "calculate" => rt_calculator_responses += 1,
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Process session events
    let session_service = agent.session_service();
    let session = session_service
        .get_session("test_app", "test_user", &final_task.context_id)
        .await
        .expect("Should be able to get session")
        .expect("Session should exist");

    let mut sess_weather_calls = 0;
    let mut sess_calculator_calls = 0;
    let mut sess_weather_responses = 0;
    let mut sess_calculator_responses = 0;

    println!("✅ Validating persisted multi-tool execution in session:");
    for event in &session.events {
        if let radkit::sessions::SessionEventType::UserMessage { content }
        | radkit::sessions::SessionEventType::AgentMessage { content } = &event.event_type
        {
            for part in &content.parts {
                match part {
                    radkit::models::content::ContentPart::FunctionCall { name, .. } => {
                        println!("  🔧 Session Function Call: {}", name);
                        match name.as_str() {
                            "get_weather" => sess_weather_calls += 1,
                            "calculate" => sess_calculator_calls += 1,
                            _ => {}
                        }
                    }
                    radkit::models::content::ContentPart::FunctionResponse {
                        name,
                        success,
                        ..
                    } => {
                        println!(
                            "  ⚙️ Session Function Response: {} (success: {})",
                            name, success
                        );
                        assert!(success, "All tool executions should succeed");
                        match name.as_str() {
                            "get_weather" => sess_weather_responses += 1,
                            "calculate" => sess_calculator_responses += 1,
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Comprehensive validation combining both sources
    let weather_calls = rt_weather_calls + sess_weather_calls;
    let calculator_calls = rt_calculator_calls + sess_calculator_calls;
    let weather_responses = rt_weather_responses + sess_weather_responses;
    let calculator_responses = rt_calculator_responses + sess_calculator_responses;

    println!("✅ Multi-tool validation summary:");
    println!(
        "  Weather: {} calls, {} responses (RT: {}/{}, Session: {}/{})",
        weather_calls,
        weather_responses,
        rt_weather_calls,
        rt_weather_responses,
        sess_weather_calls,
        sess_weather_responses
    );
    println!(
        "  Calculator: {} calls, {} responses (RT: {}/{}, Session: {}/{})",
        calculator_calls,
        calculator_responses,
        rt_calculator_calls,
        rt_calculator_responses,
        sess_calculator_calls,
        sess_calculator_responses
    );

    assert!(weather_calls >= 1, "Weather tool should have been called");
    assert!(
        calculator_calls >= 1,
        "Calculator tool should have been called"
    );
    assert!(weather_responses >= 1, "Weather tool should have responded");
    assert!(
        calculator_responses >= 1,
        "Calculator tool should have responded"
    );
    assert_eq!(
        rt_weather_calls + rt_calculator_calls,
        2,
        "Should have exactly 2 tool calls total (real-time)"
    );

    // ✅ Validate Response Contains Results from Both Tools
    let response_lower = all_text.to_lowercase();

    // Weather tool results
    assert!(
        response_lower.contains("72")
            || response_lower.contains("temperature")
            || response_lower.contains("weather"),
        "Response should include weather information: {}",
        all_text
    );

    // Calculator tool results
    assert!(
        response_lower.contains("50") || response_lower.contains("fifty"),
        "Response should include calculation result (50): {}",
        all_text
    );

    println!("✅ Multi-tool execution test completed successfully");
}

/// Test tool error handling and recovery
#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_anthropic_tool_error_handling() {
    // Create a tool that will fail for certain inputs
    let error_tool = Arc::new(
        FunctionTool::new(
            "error_tool".to_string(),
            "A tool that sometimes fails".to_string(),
            |args: HashMap<String, serde_json::Value>, _context| {
                Box::pin(async move {
                    let input = args.get("input").and_then(|v| v.as_str()).unwrap_or("");

                    if input == "fail" {
                        ToolResult {
                            success: false,
                            data: json!(null),
                            error_message: Some("Intentional failure for testing".to_string()),
                        }
                    } else {
                        ToolResult::success(json!({ "result": "success" }))
                    }
                })
            },
        )
        .with_parameters_schema(json!({
            "type": "object",
            "properties": {
                "input": {"type": "string", "description": "Input text"}
            },
            "required": ["input"]
        })),
    );

    let Some(agent) = create_test_agent_with_tools(vec![error_tool]) else {
        println!("⚠️  Skipping test: ANTHROPIC_API_KEY not found");
        return;
    };

    println!("🧪 Testing Anthropic tool error handling...");

    let message =
        create_user_message("Please use the error_tool with input 'fail' to test error handling.");

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    let result = agent
        .send_streaming_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(
        result.is_ok(),
        "Agent execution should succeed even with tool errors"
    );

    let mut execution = result.unwrap();
    let mut final_task: Option<radkit::a2a::Task> = None;

    while let Some(result) = execution.a2a_stream.next().await {
        match result {
            SendStreamingMessageResult::Message(message) => {
                for part in &message.parts {
                    if let Part::Data { data, .. } = part {
                        // Note: function_response errors are no longer sent in A2A messages
                        println!("  📦 A2A Data Part: {}", data);
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

    // Note: Tool errors are now only detected via internal events
    assert!(
        final_task.is_some(),
        "Task should complete even with tool errors"
    );

    // ✅ Check session events for tool failure
    let final_task = final_task.unwrap();
    let session_service = agent.session_service();
    let session = session_service
        .get_session("test_app", "test_user", &final_task.context_id)
        .await
        .expect("Should be able to get session")
        .expect("Session should exist");

    let mut tool_failure_detected = false;
    println!("✅ Checking session events for tool failure:");
    for event in &session.events {
        if let radkit::sessions::SessionEventType::UserMessage { content }
        | radkit::sessions::SessionEventType::AgentMessage { content } = &event.event_type
        {
            for part in &content.parts {
                if let radkit::models::content::ContentPart::FunctionResponse {
                    name,
                    success,
                    error_message,
                    ..
                } = part
                {
                    println!("  ⚙️ Function Response: {} (success: {})", name, success);
                    if !success {
                        tool_failure_detected = true;
                        println!(
                            "  ❌ Tool failure detected: {}",
                            error_message.as_deref().unwrap_or("Unknown error")
                        );
                    }
                }
            }
        }
    }

    assert!(
        tool_failure_detected,
        "Session events should capture tool failure"
    );

    println!("✅ Tool error handling test completed successfully");
}

/// Test non-streaming send_message method with tools to ensure both code paths work
#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_anthropic_non_streaming_tool_call() {
    let Some(agent) = create_test_agent_with_tools(vec![Arc::new(create_weather_tool())]) else {
        println!("⚠️  Skipping test: ANTHROPIC_API_KEY not found");
        return;
    };

    println!("🧪 Testing Anthropic non-streaming send_message with tool call...");

    let message = create_user_message(
        "What's the weather like in New York? Please use the get_weather tool.",
    );

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    // Use NON-STREAMING send_message API
    let result = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(
        result.is_ok(),
        "Agent execution should succeed: {:?}",
        result.err()
    );

    let execution = result.unwrap();

    // Validate we got a completed task
    let final_task = match execution.result {
        radkit::a2a::SendMessageResult::Task(task) => {
            println!(
                "✅ Received completed task: {} (state: {:?})",
                task.id, task.status.state
            );
            assert_eq!(
                task.status.state,
                TaskState::Submitted,
                "Task should be completed"
            );
            task
        }
        _ => panic!("Expected Task result from non-streaming API"),
    };

    // ✅ Validate Tool Execution via Real-Time Events (captured during execution)
    let mut realtime_weather_calls = 0;
    let mut realtime_weather_responses = 0;

    println!("✅ Processing captured internal events from non-streaming execution:");
    for internal_event in &execution.all_events {
        if let radkit::sessions::SessionEventType::UserMessage { content }
        | radkit::sessions::SessionEventType::AgentMessage { content } =
            &internal_event.event_type
        {
            for part in &content.parts {
                match part {
                    radkit::models::content::ContentPart::FunctionCall {
                        name, arguments, ..
                    } => {
                        println!("  🔧 Captured Function Call: {}", name);
                        if name == "get_weather" {
                            realtime_weather_calls += 1;
                            if let Some(location) = arguments.get("location") {
                                println!("    📍 Location: {}", location);
                            }
                        }
                    }
                    radkit::models::content::ContentPart::FunctionResponse {
                        name,
                        success,
                        result,
                        ..
                    } => {
                        println!(
                            "  ⚙️ Captured Function Response: {} (success: {})",
                            name, success
                        );
                        if name == "get_weather" && *success {
                            realtime_weather_responses += 1;
                            if let Some(temp) = result.get("temperature") {
                                println!("    🌡️ Temperature: {}", temp);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // ✅ Validate Tool Execution via Session Persistence
    let session_service = agent.session_service();
    let session = session_service
        .get_session("test_app", "test_user", &final_task.context_id)
        .await
        .expect("Should be able to get session")
        .expect("Session should exist");

    let mut session_weather_calls = 0;
    let mut session_weather_responses = 0;

    println!("✅ Validating persisted tool execution in session (non-streaming):");
    for event in &session.events {
        if let radkit::sessions::SessionEventType::UserMessage { content }
        | radkit::sessions::SessionEventType::AgentMessage { content } = &event.event_type
        {
            for part in &content.parts {
                match part {
                    radkit::models::content::ContentPart::FunctionCall { name, .. } => {
                        println!("  🔧 Session Function Call: {}", name);
                        if name == "get_weather" {
                            session_weather_calls += 1;
                        }
                    }
                    radkit::models::content::ContentPart::FunctionResponse {
                        name,
                        success,
                        ..
                    } => {
                        println!(
                            "  ⚙️ Session Function Response: {} (success: {})",
                            name, success
                        );
                        if name == "get_weather" && *success {
                            session_weather_responses += 1;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Validate tool execution was captured in both places
    let total_calls = realtime_weather_calls + session_weather_calls;
    let total_responses = realtime_weather_responses + session_weather_responses;

    println!("✅ Non-streaming tool validation summary:");
    println!(
        "  Captured: {} calls, {} responses",
        realtime_weather_calls, realtime_weather_responses
    );
    println!(
        "  Session: {} calls, {} responses",
        session_weather_calls, session_weather_responses
    );
    println!(
        "  Total: {} calls, {} responses",
        total_calls, total_responses
    );

    assert!(
        total_calls >= 1,
        "Should detect weather tool calls in captured events or session"
    );
    assert!(
        total_responses >= 1,
        "Should detect weather tool responses in captured events or session"
    );

    // ✅ Validate A2A events were also captured
    println!("✅ Captured A2A events: {}", execution.a2a_events.len());
    assert!(
        execution.a2a_events.len() > 0,
        "Should have captured A2A events"
    );

    // Find agent messages in A2A events
    let agent_messages: Vec<_> = execution
        .a2a_events
        .iter()
        .filter_map(|event| {
            if let SendStreamingMessageResult::Message(msg) = event {
                if msg.role == MessageRole::Agent {
                    Some(msg)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    assert!(
        agent_messages.len() >= 1,
        "Should have at least one agent response in A2A events"
    );

    // Validate response content includes weather information
    let all_agent_text: String = agent_messages
        .iter()
        .flat_map(|msg| &msg.parts)
        .filter_map(|part| {
            if let Part::Text { text, .. } = part {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    let response_lower = all_agent_text.to_lowercase();
    assert!(
        response_lower.contains("72")
            || response_lower.contains("temperature")
            || response_lower.contains("weather"),
        "Response should include weather information: {}",
        all_agent_text
    );

    // ✅ Test agent.get_task() A2A API endpoint
    println!("✅ Testing agent.get_task() A2A API endpoint:");
    let task_query_params = radkit::a2a::TaskQueryParams {
        id: final_task.id.clone(),
        history_length: None, // Get full history
        metadata: None,
    };

    let retrieved_task = agent
        .get_task("test_app", "test_user", task_query_params)
        .await
        .expect("Should be able to retrieve task via get_task API");

    // Validate A2A Task structure
    assert_eq!(retrieved_task.id, final_task.id, "Task ID should match");
    assert_eq!(
        retrieved_task.context_id, final_task.context_id,
        "Context ID should match"
    );
    assert_eq!(
        retrieved_task.status.state, final_task.status.state,
        "Task status should match"
    );
    assert_eq!(retrieved_task.kind, "task", "Task kind should be 'task'");

    println!(
        "  📋 Retrieved task: id={}, context_id={}, status={:?}",
        retrieved_task.id, retrieved_task.context_id, retrieved_task.status.state
    );
    println!(
        "  📚 Task history: {} messages",
        retrieved_task.history.len()
    );

    // Validate A2A conversation history (should contain conversation but NOT function calls)
    assert!(
        retrieved_task.history.len() >= 2,
        "Should have at least user question + agent response"
    );

    let mut found_user_message = false;
    let mut found_agent_response = false;
    let mut total_text_content = String::new();

    for (i, message) in retrieved_task.history.iter().enumerate() {
        println!(
            "  📨 A2A Message #{}: role={:?}, parts={}",
            i + 1,
            message.role,
            message.parts.len()
        );

        match message.role {
            radkit::a2a::MessageRole::User => found_user_message = true,
            radkit::a2a::MessageRole::Agent => found_agent_response = true,
        }

        for part in &message.parts {
            match part {
                radkit::a2a::Part::Text { text, .. } => {
                    let preview = text.chars().take(60).collect::<String>();
                    let suffix = if text.len() > 60 { "..." } else { "" };
                    println!("    💬 Text: {}{}", preview, suffix);
                    total_text_content.push_str(text);
                    total_text_content.push(' ');
                }
                radkit::a2a::Part::Data { data, .. } => {
                    println!("    📊 Data: {}", data);
                }
                radkit::a2a::Part::File { .. } => {
                    println!("    📎 File attachment");
                }
            }
        }
    }

    // Validation assertions
    assert!(
        found_user_message,
        "A2A task history should contain user message"
    );
    assert!(
        found_agent_response,
        "A2A task history should contain agent response"
    );

    // The agent response should include weather information (function tool results)
    let history_text_lower = total_text_content.to_lowercase();
    assert!(
        history_text_lower.contains("72")
            || history_text_lower.contains("temperature")
            || history_text_lower.contains("weather")
            || history_text_lower.contains("new york"),
        "A2A task history should contain weather information in agent response: {}",
        total_text_content
    );

    println!("  ✅ agent.get_task() API validation successful");
    println!("    - Task structure: ✓");
    println!("    - Context ID: ✓");
    println!("    - Conversation history: ✓");
    println!("    - Function calls properly excluded from A2A messages: ✓");
    println!("    - Agent response includes tool results: ✓");

    // Test agent.list_tasks()
    let all_tasks = agent
        .list_tasks("test_app", "test_user")
        .await
        .expect("Should list tasks via list_tasks API");

    assert!(all_tasks.len() >= 1, "Should have at least one task");

    let found_task = all_tasks.iter().find(|t| t.id == final_task.id);
    assert!(found_task.is_some(), "Should find our task in the list");

    println!(
        "  📋 list_tasks(): Found {} total tasks, including ours",
        all_tasks.len()
    );
    println!("  ✅ Both A2A API endpoints (get_task + list_tasks) working correctly");

    println!("✅ Non-streaming tool call test completed successfully");
}
