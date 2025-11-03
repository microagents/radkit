---
title: Events
description: Understanding events and message roles in conversation threads.
---



An `Event` represents a single message from a specific sender in a conversation `Thread`. Each event has a `Role` and `Content`.

## Roles

The `Role` indicates who sent the message. Radkit defines the following roles:

-   `Role::System`: A system prompt used to instruct the assistant.
-   `Role::User`: A message from the end-user.
-   `Role::Assistant`: A response from the LLM.
-   `Role::Tool`: A response from a tool that was called by the assistant.

## Creating Events

You can easily create events for each role.

```rust
use radkit::models::{Event, Content};

// A system instruction
let system_event = Event::system("You are a helpful assistant.");

// A user's question
let user_event = Event::user("What is Rust?");

// An assistant's response
let assistant_event = Event::assistant("Rust is a systems programming language...");

// A tool's output
let tool_response_content = Content::from_text("{\"temperature\": 72}");
let tool_event = Event::tool(tool_response_content, "weather_tool_call_id");
```

## Working with Events

You can access an event's properties to understand its role and content.

```rust
use radkit::models::{Event, Role};

fn process_event(event: &Event) {
    match event.role() {
        Role::System => println!("This is a system instruction."),
        Role::User => println!("A user said: '{}'", event.content().first_text().unwrap_or("")),
        Role::Assistant => println!("The assistant replied: '{}'", event.content().first_text().unwrap_or("")),
        Role::Tool => println!("A tool returned a result."),
    }
}
```

Events are the fundamental units that make up a `Thread`, providing the structure and history of a conversation.