# Session Management Guide

Sessions in Radkit provide sophisticated state management with three-tier isolation, automatic preference merging, and complete multi-tenant security. This guide covers everything you need to know about managing conversation state in your agents.

## Understanding Sessions

A **Session** represents a conversation context that:
- Maps to the A2A protocol's `contextId`
- Groups related tasks and interactions
- Maintains state across multiple message exchanges
- Provides automatic preference inheritance from app and user levels

### Event-Driven Conversation Architecture

**Key Concept**: Radkit sessions store **events** rather than static message history. When an agent needs to understand conversation context, it dynamically reconstructs the full conversation from `session.events`.

```rust
// From EventProcessor - how conversations are built:
async fn get_llm_conversation(
    &self,
    app_name: &str,
    user_id: &str,
    context_id: &str,
) -> AgentResult<Vec<Content>> {
    let session = self.session_service
        .get_session(app_name, user_id, context_id)
        .await?
        .ok_or_else(|| AgentError::SessionNotFound { 
            app_name: app_name.to_string(),
            user_id: user_id.to_string(),
            session_id: context_id.to_string(),
        })?;

    let mut content_messages = Vec::new();

    for event in &session.events {
        match &event.event_type {
            SessionEventType::UserMessage { content } |
            SessionEventType::AgentMessage { content } => {
                // Each message event becomes part of conversation
                content_messages.push(content.clone());
            }
            _ => {
                // Other events (StateChange, TaskCreated) don't affect conversation flow
            }
        }
    }

    Ok(content_messages)
}
```

This event-driven approach means:
- **Cross-Task Memory**: Conversations span multiple tasks within the same session
- **Rich Context**: Function calls, responses, and metadata are preserved
- **A2A Native**: Session ID maps directly to A2A `contextId` 
- **Efficient Storage**: Only events are stored, messages are reconstructed as needed

## Basic Session Operations

### Creating a New Session

Sessions are created automatically when you send a message without a `context_id`:

```rust
use radkit::a2a::{Message, MessageRole, MessageSendParams, Part, SendMessageResult};
use radkit::agents::Agent;

// Message without context_id creates a new session
let params = MessageSendParams {
    message: Message {
        kind: "message".to_string(),
        message_id: uuid::Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: "Start a new conversation".to_string(),
            metadata: None,
        }],
        context_id: None,  // ← New session will be created
        task_id: None,
        reference_task_ids: Vec::new(),
        extensions: Vec::new(),
        metadata: None,
    },
    configuration: None,
    metadata: None,
};

let response = agent.send_message(
    "my_app".to_string(),
    "user123".to_string(),
    params,
).await?;

// The created session ID is in the task's context_id
if let SendMessageResult::Task(task) = response.result {
    println!("New session created: {}", task.context_id);
}
```

### Continuing an Existing Session

To continue a conversation in the same session:

```rust
// Continue in the same session
let mut params = create_message_params("Follow-up question");
params.message.context_id = Some("existing_session_id".to_string());

let response = agent.send_message(
    "my_app".to_string(),
    "user123".to_string(),
    params,
).await?;
```

## Three-Tier State Architecture [ WORK IN PROGRESS]

Radkit implements a powerful three-tier state management system:

```
┌─────────────────────────────────────┐
│         App-Level State             │
│   (Shared across all users)         │
│   Examples: max_tokens, api_config  │
└─────────────────────────────────────┘
                 ↓
┌─────────────────────────────────────┐
│        User-Level State             │
│    (Per-user preferences)           │
│   Examples: language, timezone      │
└─────────────────────────────────────┘
                 ↓
┌─────────────────────────────────────┐
│       Session-Level State           │
│   (Conversation-specific)           │
│   Examples: topic, temp_data        │
└─────────────────────────────────────┘
```

### State Merging

When you retrieve a session, states are automatically merged:
- Session state takes priority (most specific)
- User state overrides app state
- App state provides defaults