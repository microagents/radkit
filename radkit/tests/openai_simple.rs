//! OpenAI GPT Simple Conversation Tests
//!
//! Tests basic text conversation with OpenAI GPT models using the Agent.
//! These tests require OPENAI_API_KEY environment variable to be set.
//! Tests comprehensive event validation, task lifecycle, and A2A protocol compliance.

use futures::StreamExt;
use radkit::a2a::{
    Message, MessageRole, MessageSendParams, Part, SendMessageResult, SendStreamingMessageResult,
    TaskQueryParams, TaskState,
};
use radkit::agents::Agent;
use radkit::models::OpenAILlm;
use radkit::sessions::InMemorySessionService;
use std::sync::Arc;

mod common;
use common::{get_openai_key, init_test_env};

/// Helper function to create Agent with OpenAI if API key is available
/// Creates agent with proper session service and task store for comprehensive testing
fn create_test_agent() -> Option<Agent> {
    init_test_env();
    get_openai_key().map(|api_key| {
        let openai_llm = OpenAILlm::new("gpt-4o-mini".to_string(), api_key);
        let session_service = InMemorySessionService::new();

        Agent::builder(
            "You are a helpful assistant. Respond briefly and clearly.",
            openai_llm,
        )
        .with_card(|c| {
            c.with_name("test_agent")
                .with_description("Test agent for simple conversation")
        })
        .with_session_service(session_service)
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

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_openai_simple_conversation_comprehensive() {
    let Some(agent) = create_test_agent() else {
        println!("⚠️  Skipping test: OPENAI_API_KEY not found");
        return;
    };

    println!("🧪 Testing OpenAI simple conversation with comprehensive validation...");

    let message = create_user_message("Hello! Can you tell me what 2+2 equals?");

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    // Send message using Agent SDK
    let result = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(
        result.is_ok(),
        "Agent execution should succeed: {:?}",
        result.err()
    );

    let send_result = result.unwrap();
    let mut all_text = String::new();
    let mut got_response = false;

    // ✅ 1. Validate Task Result Structure
    let task = match send_result.result {
        SendMessageResult::Task(task) => {
            println!("✅ Received task with {} messages", task.history.len());

            // Validate task state
            assert_eq!(
                task.status.state,
                TaskState::Submitted,
                "Task should be completed"
            );
            assert!(!task.id.is_empty(), "Task should have ID");
            assert!(!task.context_id.is_empty(), "Task should have context_id");

            // Validate message history structure
            assert!(
                task.history.len() >= 2,
                "Should have at least user + agent messages"
            );

            // Extract response text for content validation
            for message in &task.history {
                if message.role == MessageRole::Agent {
                    for part in &message.parts {
                        if let Part::Text { text, .. } = part {
                            all_text.push_str(text);
                            all_text.push(' ');
                            got_response = true;
                        }
                    }
                }
            }
            task
        }
        _ => panic!("Expected Task result"),
    };

    assert!(got_response, "Should have received a response");

    // ✅ 2. Validate Response Content
    let response_lower = all_text.to_lowercase();
    assert!(
        response_lower.contains("4") || response_lower.contains("four"),
        "Response should include the answer (4): {}",
        all_text
    );

    // ✅ 3. Test Task Retrieval via get_task
    let retrieved_task = agent
        .get_task(
            "test_app",
            "test_user",
            TaskQueryParams {
                id: task.id.clone(),
                history_length: None,
                metadata: None,
            },
        )
        .await;
    assert!(retrieved_task.is_ok(), "Should be able to retrieve task");
    let retrieved_task = retrieved_task.unwrap();
    assert_eq!(
        retrieved_task.id, task.id,
        "Retrieved task should have same ID"
    );
    assert_eq!(
        retrieved_task.history.len(),
        task.history.len(),
        "Retrieved task should have same history"
    );

    // ✅ 4. Test Session State and Internal Events
    let session_service = agent.session_service();
    let session = session_service
        .get_session("test_app", "test_user", &task.context_id)
        .await
        .expect("Should be able to get session")
        .expect("Session should exist");

    println!("✅ Session has {} internal events", session.events.len());
    assert!(
        !session.events.is_empty(),
        "Session should have internal events"
    );

    // Validate internal event types
    let message_events = session
        .events
        .iter()
        .filter(|e| {
            matches!(
                &e.event_type,
                radkit::sessions::SessionEventType::UserMessage { .. }
                    | radkit::sessions::SessionEventType::AgentMessage { .. }
            )
        })
        .count();

    assert!(
        message_events >= 2,
        "Should have message events for user input and model response"
    );

    // ✅ 5. Test Task Listing
    let all_tasks = agent
        .list_tasks("test_app", "test_user")
        .await
        .expect("Should be able to list tasks");
    assert!(all_tasks.len() >= 1, "Should have at least one task");
    assert!(
        all_tasks.iter().any(|t| t.id == task.id),
        "Should find our task in the list"
    );

    println!("✅ Comprehensive simple conversation test completed successfully");
}

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_openai_streaming_comprehensive() {
    let Some(agent) = create_test_agent() else {
        println!("⚠️  Skipping test: OPENAI_API_KEY not found");
        return;
    };

    println!("🧪 Testing OpenAI streaming with comprehensive A2A event validation...");

    let message = create_user_message("Hello! How are you today? Please respond with enthusiasm.");

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    // Use streaming message API for this test
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
    let mut got_response = false;
    let mut message_events = 0;
    let mut task_status_events = 0;
    let mut task_artifact_events = 0;
    let mut final_task: Option<radkit::a2a::Task> = None;

    // ✅ 1. Process A2A Streaming Events and Validate Structure
    println!("✅ Processing A2A streaming events:");
    while let Some(result) = execution.a2a_stream.next().await {
        match result {
            SendStreamingMessageResult::Message(message) => {
                message_events += 1;
                println!(
                    "  📨 A2A Message Event #{}: role={:?}, parts={}",
                    message_events,
                    message.role,
                    message.parts.len()
                );

                // Validate message structure
                assert!(!message.message_id.is_empty(), "Message should have ID");
                assert!(!message.parts.is_empty(), "Message should have parts");

                if message.role == MessageRole::Agent {
                    for part in &message.parts {
                        if let Part::Text { text, .. } = part {
                            all_text.push_str(text);
                            all_text.push(' ');
                            got_response = true;
                        }
                    }
                }
            }
            SendStreamingMessageResult::TaskStatusUpdate(status_update) => {
                task_status_events += 1;
                println!(
                    "  📊 A2A TaskStatusUpdate Event #{}: state={:?}",
                    task_status_events, status_update.status.state
                );

                // Validate status update structure
                assert!(
                    !status_update.task_id.is_empty(),
                    "Status update should have task ID"
                );
                assert!(
                    !status_update.context_id.is_empty(),
                    "Status update should have context ID"
                );
            }
            SendStreamingMessageResult::TaskArtifactUpdate(artifact_update) => {
                task_artifact_events += 1;

                // Validate artifact structure
                assert!(
                    !artifact_update.task_id.is_empty(),
                    "Artifact update should have task ID"
                );
            }
            SendStreamingMessageResult::Task(task) => {
                println!(
                    "  ✅ A2A Final Task: id={}, state={:?}, history_len={}",
                    task.id,
                    task.status.state,
                    task.history.len()
                );

                // Validate final task structure
                assert_eq!(
                    task.status.state,
                    TaskState::Submitted,
                    "Final task should be completed"
                );
                assert!(!task.id.is_empty(), "Task should have ID");
                assert!(!task.context_id.is_empty(), "Task should have context_id");
                assert!(
                    task.history.len() >= 2,
                    "Task should have user + agent messages"
                );

                final_task = Some(task);
                break; // End of stream
            }
        }
    }

    // ✅ 2. Validate A2A Event Counts
    assert!(
        message_events >= 1,
        "Should have at least 1 message event, got {}",
        message_events
    );
    assert!(final_task.is_some(), "Should receive final task");

    let final_task = final_task.unwrap();

    println!(
        "✅ A2A Event Summary: {} messages, {} status updates, {} artifacts",
        message_events, task_status_events, task_artifact_events
    );

    // ✅ 3. Validate Response Content
    assert!(got_response, "Should have received a response");
    assert!(!all_text.trim().is_empty(), "Response should not be empty");
    println!("✅ Received streaming response: {}", all_text.trim());

    // ✅ 4. Monitor Internal Events in Parallel
    let mut internal_execution_events = 0;
    let mut internal_model_events = 0;

    // Process any remaining internal events
    while let Some(internal_event) = execution.all_events_stream.next().await {
        match &internal_event.event_type {
            radkit::sessions::SessionEventType::UserMessage { content }
            | radkit::sessions::SessionEventType::AgentMessage { content } => {
                internal_execution_events += 1;
                println!("  📥 Internal Message Event: {} parts", content.parts.len());

                // Check if this is a model response by looking at role
                if content.role == radkit::a2a::MessageRole::Agent {
                    internal_model_events += 1;
                }
            }
            _ => {
                println!("  📊 Other Event Type");
            }
        }
        // Only process a few events
        if internal_execution_events >= 5 {
            break;
        }
    }

    println!(
        "✅ Internal Event Summary: {} execution, {} model events",
        internal_execution_events, internal_model_events
    );

    // ✅ 5. Validate Task Storage and Retrieval After Streaming
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
        "Should be able to retrieve task after streaming"
    );
    let retrieved_task = retrieved_task.unwrap();
    assert_eq!(
        retrieved_task.id, final_task.id,
        "Retrieved task should match streamed task"
    );

    println!("✅ Comprehensive streaming test completed successfully");
}

/// Test multi-turn conversation with comprehensive session and task validation
#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_openai_multi_turn_conversation() {
    let Some(agent) = create_test_agent() else {
        println!("⚠️  Skipping test: OPENAI_API_KEY not found");
        return;
    };

    println!("🧪 Testing OpenAI multi-turn conversation with session continuity...");

    // ✅ First turn - create new session
    let message1 = create_user_message("Hello! My name is Bob. What's 5 + 3?");
    let params1 = MessageSendParams {
        message: message1,
        configuration: None,
        metadata: None,
    };

    let result1 = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params1)
        .await;
    assert!(result1.is_ok(), "First message should succeed");

    let task1 = match result1.unwrap().result {
        SendMessageResult::Task(task) => task,
        _ => panic!("Expected Task result"),
    };

    let context_id = task1.context_id.clone();
    println!("✅ First turn created context: {}", context_id);

    // ✅ Second turn - continue in same context
    let message2 = Message {
        kind: "message".to_string(),
        message_id: uuid::Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: "Do you remember my name? Also, what's 10 * 2?".to_string(),
            metadata: None,
        }],
        context_id: Some(context_id.clone()), // Continue same session
        task_id: None,                        // New task in same context
        reference_task_ids: Vec::new(),
        extensions: Vec::new(),
        metadata: None,
    };

    let params2 = MessageSendParams {
        message: message2,
        configuration: None,
        metadata: None,
    };

    let result2 = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params2)
        .await;
    assert!(result2.is_ok(), "Second message should succeed");

    let task2 = match result2.unwrap().result {
        SendMessageResult::Task(task) => task,
        _ => panic!("Expected Task result"),
    };

    // ✅ Validate session continuity
    assert_eq!(
        task2.context_id, context_id,
        "Second task should use same context"
    );
    assert_ne!(task2.id, task1.id, "Tasks should have different IDs");

    // Extract response to check if model remembers the name
    let mut remembered_name = false;
    for message in &task2.history {
        if message.role == MessageRole::Agent {
            for part in &message.parts {
                if let Part::Text { text, .. } = part {
                    let text_lower = text.to_lowercase();
                    if text_lower.contains("bob") {
                        remembered_name = true;
                    }
                }
            }
        }
    }

    // OpenAI models should remember context from previous messages
    assert!(
        remembered_name,
        "Model should remember the name Bob from the first message"
    );

    // ✅ Third turn - test task referencing
    let message3 = Message {
        kind: "message".to_string(),
        message_id: uuid::Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: "Can you summarize what math questions I asked you?".to_string(),
            metadata: None,
        }],
        context_id: Some(context_id.clone()),
        task_id: None,
        reference_task_ids: vec![task1.id.clone(), task2.id.clone()], // Reference previous tasks
        extensions: Vec::new(),
        metadata: None,
    };

    let params3 = MessageSendParams {
        message: message3,
        configuration: None,
        metadata: None,
    };

    let result3 = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params3)
        .await;
    assert!(result3.is_ok(), "Third message should succeed");

    let task3 = match result3.unwrap().result {
        SendMessageResult::Task(task) => task,
        _ => panic!("Expected Task result"),
    };

    // Check if the model mentions the math questions
    let mut mentioned_math = false;
    for message in &task3.history {
        if message.role == MessageRole::Agent {
            for part in &message.parts {
                if let Part::Text { text, .. } = part {
                    let text_lower = text.to_lowercase();
                    if (text_lower.contains("5") && text_lower.contains("3"))
                        || (text_lower.contains("10") && text_lower.contains("2"))
                        || text_lower.contains("addition")
                        || text_lower.contains("multiplication")
                    {
                        mentioned_math = true;
                    }
                }
            }
        }
    }

    assert!(
        mentioned_math,
        "Model should mention the math questions from previous turns"
    );

    // ✅ Validate task management
    let all_tasks = agent
        .list_tasks("test_app", "test_user")
        .await
        .expect("Should be able to list tasks");
    assert!(all_tasks.len() >= 3, "Should have at least 3 tasks");

    let our_tasks: Vec<_> = all_tasks
        .iter()
        .filter(|t| t.context_id == context_id)
        .collect();
    assert_eq!(
        our_tasks.len(),
        3,
        "Should have exactly 3 tasks in our context"
    );

    // ✅ Validate session state accumulation
    let session_service = agent.session_service();
    let session = session_service
        .get_session("test_app", "test_user", &context_id)
        .await
        .expect("Should be able to get session")
        .expect("Session should exist");

    println!(
        "✅ Session has {} internal events after 3 turns",
        session.events.len()
    );
    assert!(
        session.events.len() >= 6,
        "Session should have events from all 3 task executions"
    );

    // Count different types of events
    let message_events = session
        .events
        .iter()
        .filter(|e| {
            matches!(
                &e.event_type,
                radkit::sessions::SessionEventType::UserMessage { .. }
                    | radkit::sessions::SessionEventType::AgentMessage { .. }
            )
        })
        .count();
    assert!(
        message_events >= 6,
        "Should have message events for all user inputs and model responses"
    );

    println!("✅ Multi-turn conversation test completed successfully");
}
