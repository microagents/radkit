//! Google Gemini Multi-Turn with Tool Use Tests
//!
//! Tests conversation history with function calling using Google Gemini and Agent.
//! These tests require GEMINI_API_KEY environment variable to be set.

use futures::StreamExt;
use radkit::a2a::{Message, MessageRole, MessageSendParams, Part, SendStreamingMessageResult};
use radkit::agents::Agent;
use radkit::models::GeminiLlm;
use radkit::sessions::InMemorySessionService;
use radkit::task::InMemoryTaskStore;
use radkit::tools::{FunctionTool, ToolResult};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

mod common;
use common::get_gemini_key;

/// Helper function to create Agent with Gemini if API key is available
fn create_test_agent_with_tools(tools: Vec<Arc<FunctionTool>>) -> Option<Agent> {
    get_gemini_key().map(|api_key| {
        let gemini_llm = GeminiLlm::new("gemini-2.0-flash-exp".to_string(), api_key);
        let session_service = Arc::new(InMemorySessionService::new());
        let task_store = Arc::new(InMemoryTaskStore::new());

        let base_tools: Vec<Arc<dyn radkit::tools::BaseTool>> = tools
            .into_iter()
            .map(|tool| -> Arc<dyn radkit::tools::BaseTool> { tool })
            .collect();

        Agent::new(
            "test_agent".to_string(),
            "Test agent for Gemini multi-turn function calling".to_string(),
            "You are a helpful assistant. Use the available tools when requested by the user."
                .to_string(),
            Arc::new(gemini_llm),
        )
        .with_session_service(session_service)
        .with_task_store(task_store)
        .with_tools(base_tools)
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
async fn test_gemini_multi_turn_calculation() {
    let Some(agent) = create_test_agent_with_tools(vec![Arc::new(create_calculator_tool())]) else {
        println!("⚠️  Skipping test: GEMINI_API_KEY not found");
        return;
    };

    println!("🧪 Testing Gemini multi-turn calculation with Agent (streaming)...");

    let message = create_user_message(
        "Please calculate 2+2 using the calculator tool, then tell me what you found.",
    );

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    // Use streaming message API for multi-turn tool test
    let result = agent
        .send_streaming_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(result.is_ok(), "Agent execution should succeed");

    let mut execution = result.unwrap();
    let mut all_text = String::new();
    let mut final_task = None;

    println!("✅ Processing A2A streaming events for Gemini tool execution:");
    while let Some(result) = execution.stream.next().await {
        match result {
            SendStreamingMessageResult::Message(message) => {
                println!("  📨 A2A Message Event: role={:?}", message.role);
                for part in &message.parts {
                    if let Part::Text { text, .. } = part {
                        all_text.push_str(text);
                        all_text.push(' ');
                    }
                }
            }
            SendStreamingMessageResult::TaskStatusUpdate(status) => {
                println!("  📊 A2A TaskStatusUpdate: state={:?}", status.status);
            }
            SendStreamingMessageResult::Task(task) => {
                println!(
                    "  ✅ A2A Final Task: id={}, state={:?}",
                    task.id, task.status.state
                );
                final_task = Some(task);
                break;
            }
            _ => {}
        }
    }

    assert!(final_task.is_some(), "Should receive final task");

    // ✅ Validate Tool Execution via Real-Time + Session Events
    let final_task = final_task.unwrap();

    // Process real-time events
    let mut rt_function_calls = 0;
    let mut rt_function_responses = 0;

    println!("✅ Processing real-time internal events:");
    while let Ok(internal_event) = execution.internal_events.try_recv() {
        if let radkit::events::InternalEvent::MessageReceived { content, .. } = internal_event {
            for part in &content.parts {
                match part {
                    radkit::models::content::ContentPart::FunctionCall {
                        name, arguments, ..
                    } => {
                        println!("  🔧 Real-time Function Call: {}", name);
                        if let Some(expr) = arguments.get("expression").and_then(|v| v.as_str()) {
                            println!("    📍 Real-time Expression: \"{}\"", expr);
                            assert_eq!(expr, "2+2", "Should calculate 2+2");
                        }
                        rt_function_calls += 1;
                    }
                    radkit::models::content::ContentPart::FunctionResponse {
                        name,
                        success,
                        result,
                        ..
                    } => {
                        println!(
                            "  ⚙️ Real-time Function Response: {} (success: {})",
                            name, success
                        );
                        assert!(*success, "Tool execution should succeed");
                        if let Some(data) = result.get("result") {
                            println!("    🧮 Real-time Result: {}", data);
                            assert_eq!(data.as_i64().unwrap(), 4, "Should return 4");
                        }
                        rt_function_responses += 1;
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

    let mut sess_function_calls = 0;
    let mut sess_function_responses = 0;

    println!("✅ Validating persisted tool execution in session:");
    for event in &session.events {
        if let radkit::events::InternalEvent::MessageReceived { content, .. } = event {
            for part in &content.parts {
                match part {
                    radkit::models::content::ContentPart::FunctionCall {
                        name, arguments, ..
                    } => {
                        println!("  🔧 Session Function Call: {}", name);
                        if let Some(expr) = arguments.get("expression").and_then(|v| v.as_str()) {
                            println!("    📍 Session Expression: \"{}\"", expr);
                        }
                        sess_function_calls += 1;
                    }
                    radkit::models::content::ContentPart::FunctionResponse {
                        name,
                        success,
                        result,
                        ..
                    } => {
                        println!(
                            "  ⚙️ Session Function Response: {} (success: {})",
                            name, success
                        );
                        if let Some(data) = result.get("result") {
                            println!("    🧮 Session Result: {}", data);
                        }
                        sess_function_responses += 1;
                    }
                    _ => {}
                }
            }
        }
    }

    // Comprehensive validation
    println!("✅ Tool execution validation summary:");
    println!(
        "  Real-time: {} calls, {} responses",
        rt_function_calls, rt_function_responses
    );
    println!(
        "  Session: {} calls, {} responses",
        sess_function_calls, sess_function_responses
    );

    assert!(
        rt_function_calls >= 1,
        "Calculator tool should have been called (real-time)"
    );
    assert!(
        rt_function_responses >= 1,
        "Calculator tool should have responded (real-time)"
    );
    assert!(
        sess_function_calls >= 1,
        "Calculator tool should have been called (session)"
    );
    assert!(
        sess_function_responses >= 1,
        "Calculator tool should have responded (session)"
    );

    println!("✅ Session contains {} total events", session.events.len());

    // Validate that the final response includes the calculation result
    let response_lower = all_text.to_lowercase();
    assert!(
        response_lower.contains("4") || response_lower.contains("four"),
        "Response should include the result (4) from the calculator function: {}",
        all_text
    );

    // ✅ Test A2A API endpoints: agent.get_task() and agent.list_tasks()
    println!("✅ Testing A2A API endpoints:");

    // Test agent.get_task()
    let task_query_params = radkit::a2a::TaskQueryParams {
        id: final_task.id.clone(),
        history_length: None,
        metadata: None,
    };

    let retrieved_task = agent
        .get_task("test_app", "test_user", task_query_params)
        .await
        .expect("Should retrieve task via get_task API");

    assert_eq!(retrieved_task.id, final_task.id, "Task ID should match");
    assert_eq!(
        retrieved_task.context_id, final_task.context_id,
        "Context ID should match"
    );
    assert_eq!(retrieved_task.kind, "task", "Task kind should be 'task'");
    assert!(
        retrieved_task.history.len() >= 2,
        "Should have user + agent messages"
    );

    println!(
        "  📋 get_task(): Retrieved task with {} messages",
        retrieved_task.history.len()
    );

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
    println!("  ✅ A2A API endpoints working correctly");

    println!("✅ Comprehensive Gemini tool call test completed successfully");
}

// Additional multi-turn tests will be added later
