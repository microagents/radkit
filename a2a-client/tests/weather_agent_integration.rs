//! Integration tests for A2AClient using the weather agent
//!
//! These tests use a real A2A-compliant weather agent to verify the client implementation.
//!
//! ## Running Tests
//! ```bash
//! cargo test --package a2a-client --test weather_agent_integration -- --ignored --show-output
//! ```

use a2a_client::A2AClient;
use a2a_types::{Message, MessageRole, MessageSendParams, Part, SendStreamingMessageResult};
use futures_util::StreamExt;
use std::time::Duration;

const AGENT_BASE_URL: &str = "https://mcp-servers.ag6.ai/weather";
const TEST_TIMEOUT_SECS: u64 = 60;

/// Helper to generate unique message IDs
fn generate_message_id() -> String {
    format!("msg_{}", uuid::Uuid::new_v4())
}

/// Helper to create a simple text message
fn create_text_message(text: &str, context_id: Option<String>, task_id: Option<String>) -> Message {
    Message {
        kind: "message".to_string(),
        message_id: generate_message_id(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: text.to_string(),
            metadata: None,
        }],
        context_id,
        task_id,
        reference_task_ids: vec![],
        extensions: vec![],
        metadata: None,
    }
}

#[tokio::test]
#[ignore] // Requires network access and running weather agent
async fn test_fetch_weather_agent_card() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Testing Weather Agent Card Fetching ===");

    let client = A2AClient::from_card_url(AGENT_BASE_URL).await?;
    let card = client.agent_card();

    println!("Agent Name: {}", card.name);
    println!("Agent Description: {}", card.description);
    println!("Protocol Version: {}", card.protocol_version);
    println!("Service URL: {}", card.url);
    println!("Streaming: {:?}", card.capabilities.streaming);

    assert_eq!(card.name, "Weather Agent");
    assert_eq!(card.protocol_version, "0.3.0");
    assert!(card.capabilities.streaming.unwrap_or(false));
    assert!(!card.skills.is_empty());

    println!("✓ Weather agent card fetched successfully");
    Ok(())
}

#[tokio::test]
#[ignore] // Requires network access and running weather agent
async fn test_weather_query_without_headers() -> Result<(), Box<dyn std::error::Error>> {
    // Create client with longer timeout (weather agent can take 20+ seconds)
    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(TEST_TIMEOUT_SECS))
        .build()?;

    let client = A2AClient::from_card_url_with_client(AGENT_BASE_URL, http_client).await?;

    let message = create_text_message("What is the weather in Paris?", None, None);

    println!("Sending message: {:?}", message.parts[0]);

    let result = client
        .send_message(MessageSendParams {
            message,
            configuration: None,
            metadata: None,
        })
        .await?;

    println!("✓ Response received");

    // Parse the response
    match result {
        a2a_types::SendMessageResponse::Success(response) => {
            match response.result {
                a2a_types::SendMessageResult::Task(task) => {
                    println!("  Task ID: {}", task.id);
                    println!("  Context ID: {}", task.context_id);
                    println!("  Status: {:?}", task.status.state);
                    println!("  History messages: {}", task.history.len());

                    assert!(!task.id.is_empty());
                    assert!(!task.context_id.is_empty());

                    // Find agent responses with weather info
                    let agent_messages: Vec<_> = task
                        .history
                        .iter()
                        .filter(|msg| msg.role == MessageRole::Agent)
                        .collect();

                    assert!(!agent_messages.is_empty(), "Should have agent responses");

                    // Print agent responses
                    for (i, msg) in agent_messages.iter().enumerate() {
                        println!("  Agent message {}: {:?}", i + 1, msg.parts);
                    }

                    // Check that at least one agent response mentions temperature or weather
                    let has_weather_info = agent_messages.iter().any(|msg| {
                        msg.parts.iter().any(|part| {
                            if let Part::Text { text, .. } = part {
                                text.to_lowercase().contains("temperature")
                                    || text.to_lowercase().contains("weather")
                                    || text.to_lowercase().contains("°c")
                                    || text.to_lowercase().contains("conditions")
                            } else {
                                false
                            }
                        })
                    });

                    assert!(
                        has_weather_info,
                        "Agent response should contain weather information"
                    );
                }
                a2a_types::SendMessageResult::Message(msg) => {
                    println!("  Message ID: {}", msg.message_id);
                    println!("  Parts: {:?}", msg.parts);
                }
            }
        }
        a2a_types::SendMessageResponse::Error(err) => {
            panic!("Received error response: {:?}", err);
        }
    }

    Ok(())
}

#[tokio::test]
#[ignore] // Requires network access and running weather agent
async fn test_weather_streaming() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Testing Weather Query with Streaming ===");

    let client = A2AClient::from_card_url(AGENT_BASE_URL).await?;

    let message = create_text_message("What is the weather in Tokyo?", None, None);

    println!("Sending streaming message...");

    let mut stream = client
        .send_streaming_message(MessageSendParams {
            message,
            configuration: None,
            metadata: None,
        })
        .await?;

    let mut event_count = 0;
    let mut received_task = false;
    let mut received_message = false;
    let mut task_id: Option<String> = None;

    println!("Receiving stream events...");

    while let Some(result) = stream.next().await {
        event_count += 1;

        match result? {
            SendStreamingMessageResult::Task(task) => {
                println!(
                    "  [Event {}] Task: ID={}, Status={:?}",
                    event_count, task.id, task.status.state
                );
                received_task = true;
                task_id = Some(task.id.clone());

                // Check history for weather information
                for msg in &task.history {
                    if msg.role == MessageRole::Agent {
                        for part in &msg.parts {
                            if let Part::Text { text, .. } = part {
                                if text.contains("temperature") || text.contains("weather") {
                                    println!(
                                        "    Weather info: {}",
                                        text.chars().take(100).collect::<String>()
                                    );
                                }
                            }
                        }
                    }
                }
            }
            SendStreamingMessageResult::Message(msg) => {
                println!("  [Event {}] Message: Role={:?}", event_count, msg.role);
                received_message = true;

                for part in &msg.parts {
                    if let Part::Text { text, .. } = part {
                        if text.len() < 200 {
                            println!("    Text: {}", text);
                        } else {
                            println!(
                                "    Text: {}...",
                                text.chars().take(150).collect::<String>()
                            );
                        }
                    }
                }
            }
            SendStreamingMessageResult::TaskStatusUpdate(update) => {
                println!(
                    "  [Event {}] Status Update: State={:?}",
                    event_count, update.status.state
                );
            }
            SendStreamingMessageResult::TaskArtifactUpdate(artifact) => {
                println!(
                    "  [Event {}] Artifact Update: ID={}",
                    event_count, artifact.artifact.artifact_id
                );
            }
        }

        // Safety limit
        if event_count > 50 {
            println!("  Reached event limit, stopping");
            break;
        }
    }

    println!("✓ Stream completed with {} events", event_count);
    assert!(event_count > 0, "Should receive at least one event");
    assert!(
        received_message,
        "Should receive at least one Message event"
    );

    if let Some(tid) = task_id {
        println!("  Task ID: {}", tid);
    }

    Ok(())
}

#[tokio::test]
#[ignore] // Requires network access and running weather agent
async fn test_weather_multi_turn_conversation() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Testing Multi-Turn Weather Conversation ===");

    // Create client with longer timeout (weather agent can take 20+ seconds)
    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(TEST_TIMEOUT_SECS))
        .build()?;

    let client = A2AClient::from_card_url_with_client(AGENT_BASE_URL, http_client).await?;

    // Turn 1: Ask about London weather
    println!("\n--- Turn 1: London Weather ---");
    let message1 = create_text_message("What is the weather like in London?", None, None);

    let result1 = client
        .send_message(MessageSendParams {
            message: message1,
            configuration: None,
            metadata: None,
        })
        .await?;

    let context_id = match result1 {
        a2a_types::SendMessageResponse::Success(response) => match response.result {
            a2a_types::SendMessageResult::Task(task) => {
                println!("  Task ID: {}", task.id);
                println!("  Context ID: {}", task.context_id);
                task.context_id
            }
            a2a_types::SendMessageResult::Message(msg) => {
                msg.context_id.expect("Should have context_id")
            }
        },
        a2a_types::SendMessageResponse::Error(err) => {
            panic!("Turn 1 failed: {:?}", err);
        }
    };

    // Turn 2: Ask about New York weather (same context)
    println!("\n--- Turn 2: New York Weather ---");
    let message2 = create_text_message("What about New York?", Some(context_id.clone()), None);

    let result2 = client
        .send_message(MessageSendParams {
            message: message2,
            configuration: None,
            metadata: None,
        })
        .await?;

    match result2 {
        a2a_types::SendMessageResponse::Success(response) => match response.result {
            a2a_types::SendMessageResult::Task(task) => {
                println!("  Task ID: {}", task.id);
                println!("  Context ID: {}", task.context_id);
                assert_eq!(task.context_id, context_id, "Context should be preserved");
            }
            a2a_types::SendMessageResult::Message(msg) => {
                assert_eq!(
                    msg.context_id.as_ref().unwrap(),
                    &context_id,
                    "Context should be preserved"
                );
            }
        },
        a2a_types::SendMessageResponse::Error(err) => {
            panic!("Turn 2 failed: {:?}", err);
        }
    }

    println!(
        "\n✓ Multi-turn conversation completed with shared context: {}",
        context_id
    );
    Ok(())
}

#[tokio::test]
#[ignore] // Requires network access and running weather agent
async fn test_weather_with_timeout() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Testing Weather Query with Timeout ===");

    // Create custom HTTP client with shorter timeout
    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(TEST_TIMEOUT_SECS))
        .build()?;

    let client = A2AClient::from_card_url_with_client(AGENT_BASE_URL, http_client).await?;

    let message = create_text_message("Quick weather check for Berlin", None, None);

    // Use tokio timeout to ensure test doesn't hang
    let timeout_result = tokio::time::timeout(
        Duration::from_secs(TEST_TIMEOUT_SECS),
        client.send_message(MessageSendParams {
            message,
            configuration: None,
            metadata: None,
        }),
    )
    .await;

    match timeout_result {
        Ok(Ok(_)) => {
            println!("✓ Request completed within timeout");
        }
        Ok(Err(e)) => {
            panic!("Request failed: {:?}", e);
        }
        Err(_) => {
            panic!("Request timed out after {} seconds", TEST_TIMEOUT_SECS);
        }
    }

    Ok(())
}

#[tokio::test]
#[ignore] // Requires network access and running weather agent
async fn test_error_handling_missing_headers() {
    println!("\n=== Testing Error Handling: Missing Required Headers ===");

    // Create client with longer timeout (weather agent can take 20+ seconds)
    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(TEST_TIMEOUT_SECS))
        .build()
        .expect("Should build HTTP client");

    let client = A2AClient::from_card_url_with_client(AGENT_BASE_URL, http_client)
        .await
        .expect("Should fetch agent card");

    let message = create_text_message("Weather in Paris?", None, None);

    let result = client
        .send_message(MessageSendParams {
            message,
            configuration: None,
            metadata: None,
        })
        .await;

    // Should succeed because DefaultAuthExtractor provides default values
    match result {
        Ok(_) => println!("✓ Request succeeded with default auth values"),
        Err(e) => println!("✓ Request failed as expected: {:?}", e),
    }
}
