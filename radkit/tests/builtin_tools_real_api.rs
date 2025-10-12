//! Real API Tests for Built-in Tools
//!
//! Tests the built-in tools (update_status, save_artifact) with real LLM providers
//! to ensure they work correctly with both Anthropic and Gemini APIs and generate
//! proper A2A protocol events (TaskStatusUpdateEvent, TaskArtifactUpdateEvent).

use futures::StreamExt;
use radkit::a2a::{
    Message, MessageRole, MessageSendParams, Part, SendMessageResult, SendStreamingMessageResult,
    TaskQueryParams,
};
use radkit::agents::{Agent, AgentConfig};
use radkit::models::{AnthropicLlm, GeminiLlm};
use std::sync::Arc;

mod common;
use common::{get_anthropic_key, get_gemini_key};

/// Helper function to create Agent with built-in tools enabled
fn create_test_agent_with_builtin_tools(api_key: String, provider: &str) -> Agent {
    let config = AgentConfig::default().with_max_iterations(10);

    match provider {
        "anthropic" => {
            let anthropic_llm =
                AnthropicLlm::new("claude-3-5-sonnet-20241022".to_string(), api_key);
            Agent::builder(
                "You are a helpful assistant that can update task status and save artifacts. Use the update_status tool to update your progress as you work, and when you have final results, use the save_artifact tool to save them.",
                anthropic_llm
            )
            .with_card(|c| c
                .with_name("builtin_test_agent")
                .with_description("Built-in Tools Test Agent")
            )
            .with_config(config)
            .with_builtin_task_tools() // Enable built-in status and artifact tools
            .build()
        }
        "gemini" => {
            let gemini_llm = GeminiLlm::new("gemini-2.0-flash-exp".to_string(), api_key);
            Agent::builder(
                "You are a helpful assistant with access to task management tools. When asked to update status or save artifacts, you should use the appropriate built-in tools.",
                gemini_llm
            )
            .with_card(|c| c
                .with_name("builtin_test_agent")
                .with_description("Built-in Tool Test Agent")
            )
            .with_config(config)
            .with_builtin_task_tools() // Enable built-in status and artifact tools
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

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_anthropic_status_update() {
    let Some(api_key) = get_anthropic_key() else {
        println!("⚠️  Skipping test: ANTHROPIC_API_KEY not found");
        return;
    };

    println!("🧪 Testing Anthropic status update tool...");

    let agent = create_test_agent_with_builtin_tools(api_key, "anthropic");

    let message = create_user_message(
        "Please update the task status to 'working' with the message 'Processing user request'. Use the update_status built-in tool.",
    );

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    // Use streaming to capture A2A TaskStatusUpdateEvent
    let result = agent
        .send_streaming_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(result.is_ok(), "Agent execution should succeed");

    let mut execution = result.unwrap();
    let mut status_update_events = 0;
    let mut final_task = None;

    // Process streaming results to capture A2A events
    let mut found_working_status = false;
    while let Some(event) = execution.a2a_stream.next().await {
        match event {
            SendStreamingMessageResult::TaskStatusUpdate(status_event) => {
                status_update_events += 1;
                println!(
                    "✅ A2A TaskStatusUpdateEvent: task_id={}, state={:?}, is_final={}",
                    status_event.task_id, status_event.status.state, status_event.is_final
                );
                assert_eq!(status_event.kind, "status-update");

                // Check if this is the Working status from the tool call (skip initial Submitted)
                if status_event.status.state == radkit::a2a::TaskState::Working {
                    found_working_status = true;
                }
            }
            SendStreamingMessageResult::Task(task) => {
                println!("✅ Final task with {} messages", task.history.len());
                final_task = Some(task);
                break;
            }
            _ => {} // Skip other events
        }
    }

    // Verify we received the Working status update from the tool
    assert!(
        found_working_status,
        "Should have received Working status update from update_status tool"
    );

    let final_task = final_task.expect("Should receive final task");
    println!("✅ Final task state: {:?}", final_task.status.state);

    // Verify A2A TaskStatusUpdateEvent was emitted
    assert!(
        status_update_events > 0,
        "Should have received A2A TaskStatusUpdateEvent from update_status tool"
    );

    // Assert that the task is in working state
    assert_eq!(
        final_task.status.state,
        radkit::a2a::TaskState::Working,
        "Task should be working, not {:?}",
        final_task.status.state
    );

    // Verify update_status tool was called using internal events
    let mut internal_events = Vec::new();
    // Collect some events from the stream
    let mut collected_events = Vec::new();
    for _ in 0..10 {
        if let Some(internal_event) = execution.all_events_stream.next().await {
            collected_events.push(internal_event);
        } else {
            break;
        }
    }

    for event in collected_events {
        internal_events.push(event);
    }
    let status_update_calls = count_function_calls(&internal_events, "update_status");
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
    assert_eq!(retrieved_task.id, final_task.id);
    assert_eq!(retrieved_task.status.state, radkit::a2a::TaskState::Working);

    // Test agent.list_tasks() API
    let all_tasks = agent
        .list_tasks("test_app", "test_user")
        .await
        .expect("Should list tasks via list_tasks API");
    assert!(!all_tasks.is_empty(), "Should have at least one task");
    assert!(
        all_tasks.iter().any(|t| t.id == final_task.id),
        "Should contain our task"
    );

    println!(
        "✅ A2A TaskStatusUpdateEvent properly emitted: {} events",
        status_update_events
    );
    println!("✅ Status successfully updated to working");
    println!("✅ A2A task APIs working correctly");
    println!("✅ Test completed successfully");
}

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_anthropic_artifact_save() {
    let Some(api_key) = get_anthropic_key() else {
        println!("⚠️  Skipping test: ANTHROPIC_API_KEY not found");
        return;
    };

    println!("🧪 Testing Anthropic artifact save tool...");

    let agent = create_test_agent_with_builtin_tools(api_key, "anthropic");

    let message = create_user_message(
        "Please create a simple JSON configuration file with the content '{\"name\": \"test\", \"version\": \"1.0\"}' and save it as an artifact named 'config.json'. Use the save_artifact built-in tool.",
    );

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    // Use streaming to capture A2A TaskArtifactUpdateEvent
    let result = agent
        .send_streaming_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(result.is_ok(), "Agent execution should succeed");

    let mut execution = result.unwrap();
    let mut artifact_update_events = 0;
    let mut final_task = None;
    let mut received_artifacts = Vec::new();

    // Process streaming results to capture A2A events
    while let Some(event) = execution.a2a_stream.next().await {
        match event {
            SendStreamingMessageResult::TaskArtifactUpdate(artifact_event) => {
                artifact_update_events += 1;
                println!(
                    "✅ A2A TaskArtifactUpdateEvent: task_id={}, artifact_id={}, name={:?}",
                    artifact_event.task_id,
                    artifact_event.artifact.artifact_id,
                    artifact_event.artifact.name
                );
                assert_eq!(artifact_event.kind, "artifact-update");
                assert_eq!(
                    artifact_event.artifact.name,
                    Some("config.json".to_string())
                );
                received_artifacts.push(artifact_event.artifact);
            }
            SendStreamingMessageResult::Task(task) => {
                println!("✅ Final task with {} messages", task.history.len());
                final_task = Some(task);
                break;
            }
            _ => {} // Skip other events
        }
    }

    let final_task = final_task.expect("Should receive final task");

    // Verify A2A TaskArtifactUpdateEvent was emitted
    assert!(
        artifact_update_events > 0,
        "Should have received A2A TaskArtifactUpdateEvent from save_artifact tool"
    );
    assert!(
        !received_artifacts.is_empty(),
        "Should have received artifacts from A2A events"
    );

    // Verify artifacts were created in final task
    assert!(
        !final_task.artifacts.is_empty(),
        "Should have artifacts in final task: {:?}",
        final_task.artifacts
    );

    // Find the config.json artifact
    let config_artifact = final_task
        .artifacts
        .iter()
        .find(|artifact| artifact.name == Some("config.json".to_string()))
        .expect("Should have config.json artifact in final task");

    // Verify artifact content (A2A artifacts have parts, not content field)
    assert!(
        config_artifact.parts.len() > 0,
        "Artifact should have content parts"
    );
    let content = match &config_artifact.parts[0] {
        radkit::a2a::Part::Text { text, .. } => text,
        _ => panic!("Expected text content in artifact"),
    };
    assert!(content.contains("name"));
    assert!(content.contains("test"));
    assert!(content.contains("version"));
    assert!(content.contains("1.0"));

    // Also verify A2A artifact event content matches
    let a2a_artifact = &received_artifacts[0];
    let a2a_content = match &a2a_artifact.parts[0] {
        radkit::a2a::Part::Text { text, .. } => text,
        _ => panic!("Expected text content in A2A artifact"),
    };
    assert_eq!(
        content, a2a_content,
        "A2A artifact content should match final task artifact content"
    );

    // Verify save_artifact tool was called using internal events
    let mut internal_events = Vec::new();
    // Collect some events from the stream
    let mut collected_events = Vec::new();
    for _ in 0..10 {
        if let Some(internal_event) = execution.all_events_stream.next().await {
            collected_events.push(internal_event);
        } else {
            break;
        }
    }

    for event in collected_events {
        internal_events.push(event);
    }
    let artifact_save_calls = count_function_calls(&internal_events, "save_artifact");
    assert!(
        artifact_save_calls > 0,
        "Should have called save_artifact tool"
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
    assert_eq!(retrieved_task.artifacts.len(), final_task.artifacts.len());
    assert!(
        retrieved_task
            .artifacts
            .iter()
            .any(|a| a.name == Some("config.json".to_string()))
    );

    // Test agent.list_tasks() API
    let all_tasks = agent
        .list_tasks("test_app", "test_user")
        .await
        .expect("Should list tasks via list_tasks API");
    assert!(!all_tasks.is_empty(), "Should have at least one task");
    assert!(
        all_tasks.iter().any(|t| t.id == final_task.id),
        "Should contain our task"
    );

    println!(
        "✅ A2A TaskArtifactUpdateEvent properly emitted: {} events",
        artifact_update_events
    );
    println!(
        "✅ Artifact successfully created: {:?}",
        config_artifact.name
    );
    println!("✅ Artifact content: {}", content);
    println!("✅ A2A task APIs working correctly");
    println!("✅ Test completed successfully");
}

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_gemini_status_update() {
    let Some(api_key) = get_gemini_key() else {
        println!("⚠️  Skipping test: GEMINI_API_KEY not found");
        return;
    };

    println!("🧪 Testing Gemini status update tool...");

    let agent = create_test_agent_with_builtin_tools(api_key, "gemini");

    let message = create_user_message(
        "Please update the task status to 'completed' with the message 'Task finished successfully'. Use the update_status built-in tool.",
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

    // Assert that the task is completed
    assert_eq!(
        final_task.status.state,
        radkit::a2a::TaskState::Completed,
        "Task should be completed, not {:?}",
        final_task.status.state
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
            TaskQueryParams {
                id: final_task.id.clone(),
                history_length: None,
                metadata: None,
            },
        )
        .await
        .expect("Should retrieve task via get_task API");
    assert_eq!(retrieved_task.id, final_task.id);
    assert_eq!(
        retrieved_task.status.state,
        radkit::a2a::TaskState::Completed
    );

    // Test agent.list_tasks() API
    let all_tasks = agent
        .list_tasks("test_app", "test_user")
        .await
        .expect("Should list tasks via list_tasks API");
    assert!(!all_tasks.is_empty(), "Should have at least one task");
    assert!(
        all_tasks.iter().any(|t| t.id == final_task.id),
        "Should contain our task"
    );

    println!("✅ Status successfully updated to completed");
    println!("✅ A2A task APIs working correctly");
    println!("✅ Test completed successfully");
}

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_gemini_artifact_save() {
    let Some(api_key) = get_gemini_key() else {
        println!("⚠️  Skipping test: GEMINI_API_KEY not found");
        return;
    };

    println!("🧪 Testing Gemini artifact save tool...");

    let agent = create_test_agent_with_builtin_tools(api_key, "gemini");

    let message = create_user_message(
        "Please create a Python script that prints 'Hello from Gemini!' and save it as an artifact named 'hello.py'. Use the save_artifact built-in tool.",
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

    // Verify artifacts were created
    assert!(
        !final_task.artifacts.is_empty(),
        "Should have artifacts: {:?}",
        final_task.artifacts
    );

    // Find the hello.py artifact
    let python_artifact = final_task
        .artifacts
        .iter()
        .find(|artifact| artifact.name == Some("hello.py".to_string()))
        .expect("Should have hello.py artifact");

    // Verify artifact content contains expected Python code (A2A artifacts have parts, not content field)
    let content = match &python_artifact.parts[0] {
        radkit::a2a::Part::Text { text, .. } => text,
        _ => panic!("Expected text content in artifact"),
    };
    assert!(content.contains("print"));
    assert!(content.contains("Hello from Gemini"));

    // Verify save_artifact tool was called using internal events
    let artifact_save_calls = count_function_calls(&send_result.all_events, "save_artifact");
    assert!(
        artifact_save_calls > 0,
        "Should have called save_artifact tool"
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
    assert_eq!(retrieved_task.artifacts.len(), final_task.artifacts.len());
    assert!(
        retrieved_task
            .artifacts
            .iter()
            .any(|a| a.name == Some("hello.py".to_string()))
    );

    // Test agent.list_tasks() API
    let all_tasks = agent
        .list_tasks("test_app", "test_user")
        .await
        .expect("Should list tasks via list_tasks API");
    assert!(!all_tasks.is_empty(), "Should have at least one task");
    assert!(
        all_tasks.iter().any(|t| t.id == final_task.id),
        "Should contain our task"
    );

    println!(
        "✅ Artifact successfully created: {:?}",
        python_artifact.name
    );
    println!("✅ Artifact content: {}", content);
    println!("✅ A2A task APIs working correctly");
    println!("✅ Test completed successfully");
}

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_anthropic_multiple_operations() {
    let Some(api_key) = get_anthropic_key() else {
        println!("⚠️  Skipping test: ANTHROPIC_API_KEY not found");
        return;
    };

    println!("🧪 Testing Anthropic multiple operations (status + artifact) with A2A events...");

    let agent = create_test_agent_with_builtin_tools(api_key, "anthropic");

    let message = create_user_message(
        "Please do two things: 1) Update the task status to 'working' with message 'Starting analysis', and 2) Create and save a CSV file with headers 'Name,Age,City' and one data row 'John,30,NYC' as an artifact named 'data.csv'. Use both update_status and save_artifact tools.",
    );

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    // Use streaming to capture both A2A events
    let result = agent
        .send_streaming_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(result.is_ok(), "Agent execution should succeed");

    let mut execution = result.unwrap();
    let mut status_update_events = 0;
    let mut artifact_update_events = 0;
    let mut final_task = None;

    // Process streaming results to capture A2A events
    while let Some(event) = execution.a2a_stream.next().await {
        match event {
            SendStreamingMessageResult::TaskStatusUpdate(status_event) => {
                status_update_events += 1;
                println!(
                    "✅ A2A TaskStatusUpdateEvent: state={:?}",
                    status_event.status.state
                );
                assert_eq!(status_event.kind, "status-update");
            }
            SendStreamingMessageResult::TaskArtifactUpdate(artifact_event) => {
                artifact_update_events += 1;
                println!(
                    "✅ A2A TaskArtifactUpdateEvent: name={:?}",
                    artifact_event.artifact.name
                );
                assert_eq!(artifact_event.kind, "artifact-update");
            }
            SendStreamingMessageResult::Task(task) => {
                println!("✅ Final task with {} messages", task.history.len());
                final_task = Some(task);
                break;
            }
            _ => {} // Skip other events
        }
    }

    let final_task = final_task.expect("Should receive final task");

    println!("✅ Final task state: {:?}", final_task.status.state);

    // Verify A2A events were emitted
    assert!(
        status_update_events > 0,
        "Should have received A2A TaskStatusUpdateEvent from update_status tool"
    );
    assert!(
        artifact_update_events > 0,
        "Should have received A2A TaskArtifactUpdateEvent from save_artifact tool"
    );

    // Verify task status was updated to working
    assert_eq!(
        final_task.status.state,
        radkit::a2a::TaskState::Working,
        "Task should be working, not {:?}",
        final_task.status.state
    );

    // Verify artifacts were created
    assert!(
        !final_task.artifacts.is_empty(),
        "Should have artifacts: {:?}",
        final_task.artifacts
    );

    // Find the data.csv artifact
    let csv_artifact = final_task
        .artifacts
        .iter()
        .find(|artifact| artifact.name == Some("data.csv".to_string()))
        .expect("Should have data.csv artifact");

    // Verify CSV content (A2A artifacts have parts, not content field)
    let content = match &csv_artifact.parts[0] {
        radkit::a2a::Part::Text { text, .. } => text,
        _ => panic!("Expected text content in artifact"),
    };
    assert!(content.contains("Name,Age,City"));
    assert!(content.contains("John,30,NYC"));

    // Verify both tools were called using internal events
    let mut internal_events = Vec::new();
    // Collect some events from the stream
    let mut collected_events = Vec::new();
    for _ in 0..10 {
        if let Some(internal_event) = execution.all_events_stream.next().await {
            collected_events.push(internal_event);
        } else {
            break;
        }
    }

    for event in collected_events {
        internal_events.push(event);
    }
    let status_update_calls = count_function_calls(&internal_events, "update_status");
    let artifact_save_calls = count_function_calls(&internal_events, "save_artifact");
    assert!(
        status_update_calls > 0,
        "Should have called update_status tool"
    );
    assert!(
        artifact_save_calls > 0,
        "Should have called save_artifact tool"
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
    assert_eq!(retrieved_task.id, final_task.id);
    assert_eq!(retrieved_task.status.state, radkit::a2a::TaskState::Working);
    assert_eq!(retrieved_task.artifacts.len(), final_task.artifacts.len());

    // Test agent.list_tasks() API
    let all_tasks = agent
        .list_tasks("test_app", "test_user")
        .await
        .expect("Should list tasks via list_tasks API");
    assert!(!all_tasks.is_empty(), "Should have at least one task");
    assert!(
        all_tasks.iter().any(|t| t.id == final_task.id),
        "Should contain our task"
    );

    println!(
        "✅ A2A events properly emitted: {} status updates, {} artifact updates",
        status_update_events, artifact_update_events
    );
    println!("✅ Both status update and artifact save completed");
    println!("✅ CSV artifact created: {}", content);
    println!("✅ A2A task APIs working correctly");
    println!("✅ Test completed successfully");
}

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_gemini_multiple_operations() {
    let Some(api_key) = get_gemini_key() else {
        println!("⚠️  Skipping test: GEMINI_API_KEY not found");
        return;
    };

    println!("🧪 Testing Gemini multiple operations (status + artifact)...");

    let agent = create_test_agent_with_builtin_tools(api_key, "gemini");

    let message = create_user_message(
        "Please do two things: 1) Update the task status to 'completed' with message 'Analysis finished', and 2) Create and save an HTML file with a simple heading '<h1>Analysis Results</h1>' as an artifact named 'report.html'. Use both update_status and save_artifact tools.",
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

    // Verify task status was updated to completed
    assert_eq!(
        final_task.status.state,
        radkit::a2a::TaskState::Completed,
        "Task should be completed, not {:?}",
        final_task.status.state
    );

    // Verify artifacts were created
    assert!(
        !final_task.artifacts.is_empty(),
        "Should have artifacts: {:?}",
        final_task.artifacts
    );

    // Find the report.html artifact
    let html_artifact = final_task
        .artifacts
        .iter()
        .find(|artifact| artifact.name == Some("report.html".to_string()))
        .expect("Should have report.html artifact");

    // Verify HTML content (A2A artifacts have parts, not content field)
    let content = match &html_artifact.parts[0] {
        radkit::a2a::Part::Text { text, .. } => text,
        _ => panic!("Expected text content in artifact"),
    };
    assert!(content.contains("<h1>"));
    assert!(content.contains("Analysis Results"));
    assert!(content.contains("</h1>"));

    // Verify both tools were called using internal events
    let status_update_calls = count_function_calls(&send_result.all_events, "update_status");
    let artifact_save_calls = count_function_calls(&send_result.all_events, "save_artifact");
    assert!(
        status_update_calls > 0,
        "Should have called update_status tool"
    );
    assert!(
        artifact_save_calls > 0,
        "Should have called save_artifact tool"
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
    assert_eq!(retrieved_task.id, final_task.id);
    assert_eq!(
        retrieved_task.status.state,
        radkit::a2a::TaskState::Completed
    );
    assert_eq!(retrieved_task.artifacts.len(), final_task.artifacts.len());

    // Test agent.list_tasks() API
    let all_tasks = agent
        .list_tasks("test_app", "test_user")
        .await
        .expect("Should list tasks via list_tasks API");
    assert!(!all_tasks.is_empty(), "Should have at least one task");
    assert!(
        all_tasks.iter().any(|t| t.id == final_task.id),
        "Should contain our task"
    );

    println!("✅ Both status update and artifact save completed");
    println!("✅ HTML artifact created: {}", content);
    println!("✅ A2A task APIs working correctly");
    println!("✅ Test completed successfully");
}
