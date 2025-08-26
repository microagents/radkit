//! Google Gemini Simple Conversation Tests
//!
//! Tests basic text conversation with Google Gemini using the Agent.
//! These tests require GEMINI_API_KEY environment variable to be set.

use futures::StreamExt;
use radkit::a2a::{
    Message, MessageRole, MessageSendParams, Part, SendMessageResult, SendStreamingMessageResult,
};
use radkit::agents::Agent;
use radkit::models::GeminiLlm;
use std::sync::Arc;

mod common;
use common::get_gemini_key;

/// Helper function to create Agent with Gemini if API key is available
fn create_test_agent() -> Option<Agent> {
    get_gemini_key().map(|api_key| {
        let gemini_llm = GeminiLlm::new("gemini-2.0-flash-exp".to_string(), api_key);

        Agent::new(
            "test_agent".to_string(),
            "Test agent for simple conversation".to_string(),
            "You are a helpful assistant. Respond briefly and clearly.".to_string(),
            Arc::new(gemini_llm),
        )
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
async fn test_gemini_simple_conversation() {
    let Some(agent) = create_test_agent() else {
        println!("⚠️  Skipping test: GEMINI_API_KEY not found");
        return;
    };

    println!("🧪 Testing Gemini simple conversation with Agent...");

    let message = create_user_message("Hello! Can you tell me what 3+3 equals?");

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
    let mut all_text = String::new();
    let mut got_response = false;

    match send_result.result {
        SendMessageResult::Task(task) => {
            println!("✅ Received task with {} messages", task.history.len());
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
        }
        _ => panic!("Expected Task result"),
    }

    assert!(got_response, "Should have received a response");

    // Validate that the response includes the answer
    let response_lower = all_text.to_lowercase();
    assert!(
        response_lower.contains("6") || response_lower.contains("six"),
        "Response should include the answer (6): {}",
        all_text
    );
}

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_gemini_creative_question() {
    let Some(agent) = create_test_agent() else {
        println!("⚠️  Skipping test: GEMINI_API_KEY not found");
        return;
    };

    println!("🧪 Testing Gemini creative question with Agent (streaming)...");

    let message = create_user_message("Tell me one interesting fact about space in one sentence.");

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    // Use streaming message API for this test
    let result = agent
        .send_streaming_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(result.is_ok(), "Agent execution should succeed");

    let mut execution = result.unwrap();
    let mut all_text = String::new();
    let mut got_response = false;

    println!("✅ Processing streaming events:");
    while let Some(result) = execution.stream.next().await {
        println!("  Streaming Result: {:?}", result);
        match result {
            SendStreamingMessageResult::Message(message) => {
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
            SendStreamingMessageResult::Task(task) => {
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
            }
            _ => {}
        }
    }

    assert!(got_response, "Should have received a response");
    assert!(!all_text.trim().is_empty(), "Response should not be empty");

    // Validate that the response is about space
    let response_lower = all_text.to_lowercase();
    assert!(
        response_lower.contains("space")
            || response_lower.contains("universe")
            || response_lower.contains("star")
            || response_lower.contains("planet")
            || response_lower.contains("galaxy")
            || response_lower.contains("cosmos"),
        "Response should be about space: {}",
        all_text
    );

    println!("✅ Received streaming space fact: {}", all_text.trim());
}

// Additional simple conversation tests will be added later
