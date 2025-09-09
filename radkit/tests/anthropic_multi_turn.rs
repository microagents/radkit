//! Anthropic Claude Multi-Turn Conversation Tests
//!
//! Tests conversation history and context retention with Anthropic Claude using Agent.
//! These tests require ANTHROPIC_API_KEY environment variable to be set.

use radkit::a2a::{Message, MessageRole, MessageSendParams, Part, SendMessageResult};
use radkit::agents::Agent;
use radkit::models::AnthropicLlm;
use std::sync::Arc;

mod common;
use common::get_anthropic_key;

/// Helper function to create Agent with Anthropic if API key is available
fn create_test_agent() -> Option<Agent> {
    get_anthropic_key().map(|api_key| {
        let anthropic_llm = AnthropicLlm::new("claude-3-5-sonnet-20241022".to_string(), api_key);

        Agent::new(
            "test_agent".to_string(),
            "Test agent for multi-turn conversation".to_string(),
            "You are a helpful assistant. Remember what we discuss and refer back to it."
                .to_string(),
            Arc::new(anthropic_llm),
        )
    })
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

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_anthropic_context_retention() {
    let Some(agent) = create_test_agent() else {
        println!("⚠️  Skipping test: ANTHROPIC_API_KEY not found");
        return;
    };

    println!("🧪 Testing Anthropic context retention with Agent...");

    // First message - establish context (let system create new session)
    let message1 = create_user_message(
        "My name is Alice and I love reading mystery novels. What's your name?",
    );

    let params1 = MessageSendParams {
        message: message1,
        configuration: None,
        metadata: None,
    };

    // Send first message using new Agent SDK format
    let result1 = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params1)
        .await;
    assert!(result1.is_ok(), "First agent execution should succeed");

    let send_result1 = result1.unwrap();
    let context_id = match &send_result1.result {
        SendMessageResult::Task(task) => {
            println!(
                "✅ First conversation: Received task with context_id: {}",
                task.context_id
            );
            task.context_id.clone()
        }
        _ => panic!("Expected Task result"),
    };

    // Second message - test context retention
    let mut message2 = create_user_message("Do you remember my name? And what do I like to read?");
    message2.context_id = Some(context_id);

    let params2 = MessageSendParams {
        message: message2,
        configuration: None,
        metadata: None,
    };

    // Send second message to same context
    let result2 = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params2)
        .await;
    assert!(result2.is_ok(), "Second agent execution should succeed");

    let send_result2 = result2.unwrap();
    let mut all_text = String::new();

    match send_result2.result {
        SendMessageResult::Task(task) => {
            println!(
                "✅ Second conversation: Received task with {} messages",
                task.history.len()
            );
            for message in &task.history {
                if message.role == MessageRole::Agent {
                    for part in &message.parts {
                        if let Part::Text { text, .. } = part {
                            all_text.push_str(text);
                            all_text.push(' ');
                        }
                    }
                }
            }
        }
        _ => panic!("Expected Task result"),
    }

    // Validate context retention
    let response_lower = all_text.to_lowercase();
    assert!(
        response_lower.contains("alice"),
        "Response should remember the name Alice: {}",
        all_text
    );
    assert!(
        response_lower.contains("mystery") || response_lower.contains("novel"),
        "Response should remember the reading preference: {}",
        all_text
    );

    println!("✅ Context successfully retained!");
}

// Additional multi-turn conversation tests will be added later
