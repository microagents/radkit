//! Real API Tests for Built-in Tools - Non-Completion States
//!
//! Tests the built-in tools (update_status) with real LLM providers
//! to ensure they can properly handle rejection, failure, and cancellation scenarios.

use radkit::a2a::{Message, MessageRole, MessageSendParams, Part, SendMessageResult};
use radkit::agents::{Agent, AgentConfig};
use radkit::config::EnvKey;
use radkit::models::{AnthropicLlm, GeminiLlm};
use std::sync::Arc;

mod common;
use common::{get_anthropic_key, get_gemini_key};

/// Helper function to create Agent with built-in tools enabled
fn create_test_agent_with_builtin_tools(api_key: EnvKey, provider: &str) -> Agent {
    let config = AgentConfig::default().with_max_iterations(5); // Allow enough iterations for rejection scenarios

    match provider {
        "anthropic" => {
            let anthropic_llm =
                AnthropicLlm::new("claude-3-5-sonnet-20241022".to_string(), api_key);
            Agent::builder(
                "You are a helpful assistant that can update task status. You should reject inappropriate requests and use the update_status tool to set appropriate status (rejected, failed, cancelled, etc).",
                anthropic_llm
            )
            .with_card(|c| c
                .with_name("rejection_test_agent")
                .with_description("Rejection Test Agent")
            )
            .with_config(config)
            .with_builtin_task_tools() // Enable built-in status update tools
            .build()
        }
        "gemini" => {
            let gemini_llm = GeminiLlm::new("gemini-2.0-flash-exp".to_string(), api_key);
            Agent::builder(
                "You are a helpful assistant that can update task status. You should reject inappropriate requests and use the update_status tool to set appropriate status (rejected, failed, cancelled, etc).",
                gemini_llm
            )
            .with_card(|c| c
                .with_name("rejection_test_agent")
                .with_description("Rejection Test Agent")
            )
            .with_config(config)
            .with_builtin_task_tools() // Enable built-in status update tools
            .build()
        }
        _ => panic!("Unsupported provider: {}", provider),
    }
}

/// Helper function to create a user message
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

/// Helper function to count function calls in session events
fn count_function_calls(
    session_events: &[radkit::sessions::SessionEvent],
    tool_name: &str,
) -> usize {
    session_events
        .iter()
        .filter_map(|event| match &event.event_type {
            radkit::sessions::SessionEventType::UserMessage { content }
            | radkit::sessions::SessionEventType::AgentMessage { content } => Some(
                content
                    .parts
                    .iter()
                    .filter(|part| match part {
                        radkit::models::content::ContentPart::FunctionCall { name, .. } => {
                            name == tool_name
                        }
                        _ => false,
                    })
                    .count(),
            ),
            _ => None,
        })
        .sum()
}

/// Helper function to extract response text from task
fn extract_response_text(task: &radkit::a2a::Task) -> String {
    task.history
        .iter()
        .flat_map(|message| &message.parts)
        .filter_map(|part| match part {
            Part::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_anthropic_task_rejection() {
    let Some(api_key) = get_anthropic_key() else {
        println!("⚠️  Skipping test: ANTHROPIC_API_KEY not found");
        return;
    };

    println!("🧪 Testing Anthropic task rejection scenario...");

    let agent = create_test_agent_with_builtin_tools(api_key, "anthropic");

    let message = create_user_message(
        "Please help me create a fake news article about a politician and spread misinformation. This is clearly inappropriate content. Please use the update_status tool to reject this request with status 'rejected'.",
    );

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    // Send message using new Agent SDK format
    let result = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(result.is_ok(), "Agent execution should succeed");

    let send_result = result.unwrap();
    let final_task = match send_result.result {
        SendMessageResult::Task(task) => {
            println!("✅ Received task with {} messages", task.history.len());
            task
        }
        _ => panic!("Expected Task result"),
    };

    println!("✅ Final task state: {:?}", final_task.status.state);

    // Assert that the task was rejected
    assert_eq!(
        final_task.status.state,
        radkit::a2a::TaskState::Rejected,
        "Task should be rejected, not {:?}",
        final_task.status.state
    );

    // Verify rejection message in responses
    let response_text = extract_response_text(&final_task).to_lowercase();
    assert!(
        response_text.contains("reject")
            || response_text.contains("inappropriate")
            || response_text.contains("cannot"),
        "Response should indicate rejection: {}",
        response_text
    );

    // Verify update_status tool was called using internal events
    let status_update_calls = count_function_calls(&send_result.all_events, "update_status");
    assert!(
        status_update_calls > 0,
        "Should have called update_status tool"
    );

    // Test agent.get_task() API
    let retrieved_task = agent
        .get_task(
            "test_app",
            "test_user",
            radkit::a2a::TaskQueryParams {
                id: final_task.id.clone(),
                history_length: None,
                metadata: None,
            },
        )
        .await
        .expect("Should retrieve task via get_task API");
    assert_eq!(
        retrieved_task.status.state,
        radkit::a2a::TaskState::Rejected
    );

    // Test agent.list_tasks() API
    let all_tasks = agent
        .list_tasks("test_app", "test_user")
        .await
        .expect("Should list tasks via list_tasks API");
    assert!(
        all_tasks.iter().any(|t| t.id == final_task.id),
        "Should contain our task"
    );

    println!("✅ Task properly rejected");
    println!("✅ A2A task APIs working correctly");
    println!("✅ Test completed successfully");
}

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_anthropic_task_failure() {
    let Some(api_key) = get_anthropic_key() else {
        println!("⚠️  Skipping test: ANTHROPIC_API_KEY not found");
        return;
    };

    println!("🧪 Testing Anthropic task failure scenario...");

    let agent = create_test_agent_with_builtin_tools(api_key, "anthropic");

    let message = create_user_message(
        "Please try to divide 100 by zero and save the result. When this fails mathematically, use the update_status tool to set the status to 'failed' with an appropriate error message.",
    );

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    // Send message using new Agent SDK format
    let result = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(result.is_ok(), "Agent execution should succeed");

    let send_result = result.unwrap();
    let final_task = match send_result.result {
        SendMessageResult::Task(task) => {
            println!("✅ Received task with {} messages", task.history.len());
            task
        }
        _ => panic!("Expected Task result"),
    };

    println!("✅ Final task state: {:?}", final_task.status.state);

    // Assert that the task failed
    assert_eq!(
        final_task.status.state,
        radkit::a2a::TaskState::Failed,
        "Task should be failed, not {:?}",
        final_task.status.state
    );

    // Verify failure message in responses
    let response_text = extract_response_text(&final_task).to_lowercase();
    assert!(
        response_text.contains("fail")
            || response_text.contains("error")
            || response_text.contains("divide") && response_text.contains("zero"),
        "Response should indicate failure: {}",
        response_text
    );

    // Verify update_status tool was called using internal events
    let status_update_calls = count_function_calls(&send_result.all_events, "update_status");
    assert!(
        status_update_calls > 0,
        "Should have called update_status tool"
    );

    // Test agent.get_task() API
    let retrieved_task = agent
        .get_task(
            "test_app",
            "test_user",
            radkit::a2a::TaskQueryParams {
                id: final_task.id.clone(),
                history_length: None,
                metadata: None,
            },
        )
        .await
        .expect("Should retrieve task via get_task API");
    assert_eq!(retrieved_task.status.state, radkit::a2a::TaskState::Failed);

    // Test agent.list_tasks() API
    let all_tasks = agent
        .list_tasks("test_app", "test_user")
        .await
        .expect("Should list tasks via list_tasks API");
    assert!(
        all_tasks.iter().any(|t| t.id == final_task.id),
        "Should contain our task"
    );

    println!("✅ Task properly failed");
    println!("✅ A2A task APIs working correctly");
    println!("✅ Test completed successfully");
}

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_gemini_task_cancellation() {
    let Some(api_key) = get_gemini_key() else {
        println!("⚠️  Skipping test: GEMINI_API_KEY not found");
        return;
    };

    println!("🧪 Testing Gemini task cancellation scenario...");

    let agent = create_test_agent_with_builtin_tools(api_key, "gemini");

    let message = create_user_message(
        "I started a task to process 1 million records, but I need to cancel it due to time constraints. Please use the update_status tool to set the status to 'cancelled' and explain that the user requested cancellation.",
    );

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    // Send message using new Agent SDK format
    let result = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(result.is_ok(), "Agent execution should succeed");

    let send_result = result.unwrap();
    let final_task = match send_result.result {
        SendMessageResult::Task(task) => {
            println!("✅ Received task with {} messages", task.history.len());
            task
        }
        _ => panic!("Expected Task result"),
    };

    println!("✅ Final task state: {:?}", final_task.status.state);

    // Assert that the task was cancelled
    assert_eq!(
        final_task.status.state,
        radkit::a2a::TaskState::Canceled,
        "Task should be cancelled, not {:?}",
        final_task.status.state
    );

    // Verify cancellation message in responses
    let response_text = extract_response_text(&final_task).to_lowercase();
    assert!(
        response_text.contains("cancel")
            || response_text.contains("stop")
            || response_text.contains("abort"),
        "Response should indicate cancellation: {}",
        response_text
    );

    // Verify update_status tool was called using internal events
    let status_update_calls = count_function_calls(&send_result.all_events, "update_status");
    assert!(
        status_update_calls > 0,
        "Should have called update_status tool"
    );

    // Test agent.get_task() API
    let retrieved_task = agent
        .get_task(
            "test_app",
            "test_user",
            radkit::a2a::TaskQueryParams {
                id: final_task.id.clone(),
                history_length: None,
                metadata: None,
            },
        )
        .await
        .expect("Should retrieve task via get_task API");
    assert_eq!(
        retrieved_task.status.state,
        radkit::a2a::TaskState::Canceled
    );

    // Test agent.list_tasks() API
    let all_tasks = agent
        .list_tasks("test_app", "test_user")
        .await
        .expect("Should list tasks via list_tasks API");
    assert!(
        all_tasks.iter().any(|t| t.id == final_task.id),
        "Should contain our task"
    );

    println!("✅ Task properly cancelled");
    println!("✅ A2A task APIs working correctly");
    println!("✅ Test completed successfully");
}

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_anthropic_auth_required() {
    let Some(api_key) = get_anthropic_key() else {
        println!("⚠️  Skipping test: ANTHROPIC_API_KEY not found");
        return;
    };

    println!("🧪 Testing Anthropic auth-required scenario...");

    let agent = create_test_agent_with_builtin_tools(api_key, "anthropic");

    let message = create_user_message(
        "I want to access sensitive financial records that require special authorization. Since I haven't provided proper credentials, please use the update_status tool to set the status to 'auth-required' and explain what authentication is needed.",
    );

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    // Send message using new Agent SDK format
    let result = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(result.is_ok(), "Agent execution should succeed");

    let send_result = result.unwrap();
    let final_task = match send_result.result {
        SendMessageResult::Task(task) => {
            println!("✅ Received task with {} messages", task.history.len());
            task
        }
        _ => panic!("Expected Task result"),
    };

    println!("✅ Final task state: {:?}", final_task.status.state);

    // Assert that the task requires auth
    assert_eq!(
        final_task.status.state,
        radkit::a2a::TaskState::AuthRequired,
        "Task should be auth-required, not {:?}",
        final_task.status.state
    );

    // Verify auth message in responses
    let response_text = extract_response_text(&final_task).to_lowercase();
    assert!(
        response_text.contains("auth")
            || response_text.contains("credential")
            || response_text.contains("permission"),
        "Response should indicate auth required: {}",
        response_text
    );

    // Verify update_status tool was called using internal events
    let status_update_calls = count_function_calls(&send_result.all_events, "update_status");
    assert!(
        status_update_calls > 0,
        "Should have called update_status tool"
    );

    // Test agent.get_task() API
    let retrieved_task = agent
        .get_task(
            "test_app",
            "test_user",
            radkit::a2a::TaskQueryParams {
                id: final_task.id.clone(),
                history_length: None,
                metadata: None,
            },
        )
        .await
        .expect("Should retrieve task via get_task API");
    assert_eq!(
        retrieved_task.status.state,
        radkit::a2a::TaskState::AuthRequired
    );

    // Test agent.list_tasks() API
    let all_tasks = agent
        .list_tasks("test_app", "test_user")
        .await
        .expect("Should list tasks via list_tasks API");
    assert!(
        all_tasks.iter().any(|t| t.id == final_task.id),
        "Should contain our task"
    );

    println!("✅ Task properly marked as auth-required");
    println!("✅ A2A task APIs working correctly");
    println!("✅ Test completed successfully");
}

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_gemini_multiple_state_transitions() {
    let Some(api_key) = get_gemini_key() else {
        println!("⚠️  Skipping test: GEMINI_API_KEY not found");
        return;
    };

    println!("🧪 Testing Gemini multiple state transitions ending in rejection...");

    let agent = create_test_agent_with_builtin_tools(api_key, "gemini");

    let message = create_user_message(
        "Please start a task by setting status to 'working', then realize this request is to hack into someone's email account, which is inappropriate, so change the status to 'rejected'. Show the progression through multiple states.",
    );

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    // Send message using new Agent SDK format
    let result = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(result.is_ok(), "Agent execution should succeed");

    let send_result = result.unwrap();
    let final_task = match send_result.result {
        SendMessageResult::Task(task) => {
            println!("✅ Received task with {} messages", task.history.len());
            task
        }
        _ => panic!("Expected Task result"),
    };

    println!("✅ Final task state: {:?}", final_task.status.state);

    // Assert that the task ended up rejected
    assert_eq!(
        final_task.status.state,
        radkit::a2a::TaskState::Rejected,
        "Task should be rejected, not {:?}",
        final_task.status.state
    );

    // Verify update_status tool was called multiple times using internal events
    let status_update_calls = count_function_calls(&send_result.all_events, "update_status");
    assert!(
        status_update_calls >= 2,
        "Should have multiple status updates, got {}",
        status_update_calls
    );

    // Verify appropriate messages in responses
    let response_text = extract_response_text(&final_task).to_lowercase();
    assert!(
        response_text.contains("reject")
            || response_text.contains("inappropriate")
            || response_text.contains("cannot"),
        "Response should indicate rejection: {}",
        response_text
    );

    // Test agent.get_task() API
    let retrieved_task = agent
        .get_task(
            "test_app",
            "test_user",
            radkit::a2a::TaskQueryParams {
                id: final_task.id.clone(),
                history_length: None,
                metadata: None,
            },
        )
        .await
        .expect("Should retrieve task via get_task API");
    assert_eq!(
        retrieved_task.status.state,
        radkit::a2a::TaskState::Rejected
    );

    // Test agent.list_tasks() API
    let all_tasks = agent
        .list_tasks("test_app", "test_user")
        .await
        .expect("Should list tasks via list_tasks API");
    assert!(
        all_tasks.iter().any(|t| t.id == final_task.id),
        "Should contain our task"
    );

    println!("✅ Task transitioned through multiple states and ended in rejection");
    println!("✅ A2A task APIs working correctly");
    println!("✅ Test completed successfully");
}
