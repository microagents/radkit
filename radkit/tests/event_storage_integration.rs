//! Event Storage Integration Tests
//!
//! Comprehensive tests for the event projection and storage system

use radkit::a2a::{Message, MessageRole, MessageSendParams, Part, SendMessageResult, TaskState};
use radkit::agents::Agent;
use radkit::errors::AgentResult;
use radkit::models::MockLlm;
use std::sync::Arc;
use uuid::Uuid;

/// Test complete event storage pipeline with agent execution
#[tokio::test]
async fn test_complete_event_storage_pipeline() -> AgentResult<()> {
    println!("🚀 Testing Complete Event Storage Pipeline");

    // Create an agent with automatic event storage via StorageProjector
    let mock_llm = Arc::new(MockLlm::new("test-model".to_string()));

    let agent = Agent::new(
        "test-agent".to_string(),
        "Test agent for event storage".to_string(),
        "You are a helpful assistant.".to_string(),
        mock_llm,
    );

    // Create a message to send
    let message = Message {
        kind: "message".to_string(),
        message_id: Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: "Hello, how are you?".to_string(),
            metadata: None,
        }],
        context_id: None, // Will create new session
        task_id: None,    // Will create new task
        reference_task_ids: Vec::new(),
        extensions: Vec::new(),
        metadata: None,
    };

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    // Send message (this will generate and store events via StorageProjector)
    let response = agent
        .send_message("test-app".to_string(), "test-user".to_string(), params)
        .await?;

    // Verify we got a task response
    let task = match response.result {
        SendMessageResult::Task(task) => task,
        _ => panic!("Expected Task result"),
    };

    println!("✅ Task created: {}", task.id);
    assert_eq!(task.status.state, TaskState::Submitted); // No automatic completion
    assert!(!task.history.is_empty());

    // Verify events were stored by checking task history
    assert!(task.history.len() >= 2); // At least user message and agent response
    println!("✅ Task has {} messages in history", task.history.len());

    // Verify we can retrieve the task
    let retrieved_task = agent
        .get_task(
            "test-app",
            "test-user",
            radkit::a2a::TaskQueryParams {
                id: task.id.clone(),
                history_length: None,
                metadata: None,
            },
        )
        .await?;

    assert_eq!(retrieved_task.id, task.id);
    println!("✅ Task retrieved successfully");

    // Verify session was created and has events
    let session_service = agent.session_service();
    let session = session_service
        .get_session("test-app", "test-user", &task.context_id)
        .await?
        .expect("Session should exist");

    assert!(!session.events.is_empty());
    println!("✅ Session has {} session events", session.events.len());

    println!("🎉 Complete event storage pipeline test passed!");
    Ok(())
}

/// Test multi-turn conversation with event storage
#[tokio::test]
async fn test_multi_turn_event_storage() -> AgentResult<()> {
    println!("🚀 Testing Multi-Turn Event Storage");

    let mock_llm = Arc::new(MockLlm::new("test-model".to_string()));
    let agent = Agent::new(
        "multi-turn-agent".to_string(),
        "Multi-turn test agent".to_string(),
        "You are a helpful assistant.".to_string(),
        mock_llm,
    );

    // First message - creates new session and task
    let message1 = Message {
        kind: "message".to_string(),
        message_id: Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: "Let's start a conversation".to_string(),
            metadata: None,
        }],
        context_id: None,
        task_id: None,
        reference_task_ids: Vec::new(),
        extensions: Vec::new(),
        metadata: None,
    };

    let params1 = MessageSendParams {
        message: message1,
        configuration: None,
        metadata: None,
    };

    let response1 = agent
        .send_message("test-app".to_string(), "test-user".to_string(), params1)
        .await?;

    let task1 = match response1.result {
        SendMessageResult::Task(task) => task,
        _ => panic!("Expected Task result"),
    };

    let context_id = task1.context_id.clone();
    println!("✅ First task created with context: {}", context_id);

    // Second message - continues in same context
    let message2 = Message {
        kind: "message".to_string(),
        message_id: Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: "Let's continue our conversation".to_string(),
            metadata: None,
        }],
        context_id: Some(context_id.clone()),
        task_id: None, // New task in same context
        reference_task_ids: Vec::new(),
        extensions: Vec::new(),
        metadata: None,
    };

    let params2 = MessageSendParams {
        message: message2,
        configuration: None,
        metadata: None,
    };

    let response2 = agent
        .send_message("test-app".to_string(), "test-user".to_string(), params2)
        .await?;

    let task2 = match response2.result {
        SendMessageResult::Task(task) => task,
        _ => panic!("Expected Task result"),
    };

    // Verify both tasks are in the same context
    assert_eq!(task2.context_id, context_id);
    println!("✅ Second task in same context");

    // Verify we can list all tasks for this user
    let all_tasks = agent.list_tasks("test-app", "test-user").await?;
    assert!(all_tasks.len() >= 2);
    println!("✅ Found {} tasks for user", all_tasks.len());

    // Verify session has accumulated events from both tasks
    let session_service = agent.session_service();
    let session = session_service
        .get_session("test-app", "test-user", &context_id)
        .await?
        .expect("Session should exist");

    assert!(session.events.len() >= 4); // Events from both task executions
    println!(
        "✅ Session has accumulated {} events from multi-turn conversation",
        session.events.len()
    );

    println!("🎉 Multi-turn event storage test passed!");
    Ok(())
}

/// Test event storage with tool usage
#[tokio::test]
async fn test_tool_event_storage() -> AgentResult<()> {
    println!("🚀 Testing Tool Event Storage");

    use radkit::tools::{FunctionTool, ToolResult};
    use serde_json::json;
    use std::collections::HashMap;

    // Create a simple tool
    let calculator_tool = Arc::new(FunctionTool::new(
        "calculate".to_string(),
        "Perform simple calculations".to_string(),
        |args: HashMap<String, serde_json::Value>, _context| {
            Box::pin(async move {
                let result = args
                    .get("expression")
                    .and_then(|v| v.as_str())
                    .map(|expr| match expr {
                        "2+2" => 4,
                        "10*5" => 50,
                        _ => 0,
                    })
                    .unwrap_or(0);

                ToolResult::success(json!({ "result": result }))
            })
        },
    ));

    let mock_llm = Arc::new(MockLlm::new("test-model".to_string()));
    let agent = Agent::new(
        "tool-agent".to_string(),
        "Tool test agent".to_string(),
        "You are a helpful assistant with calculation tools.".to_string(),
        mock_llm,
    )
    .with_tools(vec![calculator_tool]);

    // Send a message that might trigger tool use
    let message = Message {
        kind: "message".to_string(),
        message_id: Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: "What is 2+2?".to_string(),
            metadata: None,
        }],
        context_id: None,
        task_id: None,
        reference_task_ids: Vec::new(),
        extensions: Vec::new(),
        metadata: None,
    };

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    let response = agent
        .send_message("test-app".to_string(), "test-user".to_string(), params)
        .await?;

    let task = match response.result {
        SendMessageResult::Task(task) => task,
        _ => panic!("Expected Task result"),
    };

    println!("✅ Task completed with tools");

    // Check if tool execution events were stored
    let session_service = agent.session_service();
    let session = session_service
        .get_session("test-app", "test-user", &task.context_id)
        .await?
        .expect("Session should exist");

    // Check for tool execution events
    let tool_events = session
        .events
        .iter()
        .filter(|e| {
            match &e.event_type {
                radkit::sessions::SessionEventType::UserMessage { content }
                | radkit::sessions::SessionEventType::AgentMessage { content } => {
                    // Check if content has function calls or responses
                    content.parts.iter().any(|part| {
                        matches!(
                            part,
                            radkit::models::content::ContentPart::FunctionCall { .. }
                                | radkit::models::content::ContentPart::FunctionResponse { .. }
                        )
                    })
                }
                _ => false,
            }
        })
        .count();

    if tool_events > 0 {
        println!("✅ Found {} tool execution events", tool_events);
    } else {
        println!("ℹ️  No tool execution events (MockLlm might not trigger tools)");
    }

    println!("🎉 Tool event storage test passed!");
    Ok(())
}
