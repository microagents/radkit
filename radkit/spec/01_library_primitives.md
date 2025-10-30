# 01: Library Primitives

This document specifies the core data primitives that form the foundation of the `radkit` SDK. These types are used for representing conversation history, message content, and individual events.

## Core Concepts

### Thread - Conversation Context

A `Thread` represents the complete conversation history with the LLM, including system prompts and message exchanges.

```rust
use radkit::models::{Thread, Event};

// Simple thread from user message
let thread = Thread::from_user("Hello, world!");

// Thread with system prompt
let thread = Thread::from_system("You are a helpful coding assistant")
    .add_event(Event::user("Explain Rust ownership"));

// Multi-turn conversation
let thread = Thread::new(vec![
    Event::user("What is 2+2?"),
    Event::assistant("2+2 equals 4."),
    Event::user("What about 3+3?"),
]);

// Builder pattern
let thread = Thread::new(vec![])
    .with_system("You are an expert in mathematics")
    .add_event(Event::user("Calculate the area of a circle with radius 5"));
```

**Type Conversions:**

```rust
// From string slice
let thread: Thread = "Hello".into();

// From String
let thread: Thread = String::from("World").into();

// From Event
let thread: Thread = Event::user("Question").into();

// From Vec<Event>
let thread: Thread = vec![
    Event::user("First"),
    Event::assistant("Response"),
].into();
```

---

### Content - Multi-Modal Messages

`Content` represents the payload of a message, supporting text, images, documents, tool calls, and tool responses.

```rust
use radkit::models::{Content, ContentPart};
use serde_json::json;

// Simple text content
let content = Content::from_text("Hello!");

// Multi-part content
let content = Content::from_parts(vec![
    ContentPart::Text("Check this image:".to_string()),
    ContentPart::from_data(
        "image/png",
        "base64_encoded_image_data_here",
        Some("photo.png".to_string())
    )?,
]);

// Access text parts
for text in content.texts() {
    println!("{}", text);
}

// Query content
if content.has_text() {
    println!("First text: {}", content.first_text().unwrap());
}

if content.has_tool_calls() {
    println!("Tool calls: {}", content.tool_calls().len());
}

// Join all text parts
if let Some(combined) = content.joined_texts() {
    println!("Combined: {}", combined);
}
```

---

### Event - Conversation Messages

`Event` represents a single message in a conversation with an associated role.

```rust
use radkit::models::{Event, Role};

// Create events with different roles
let system_event = Event::system("You are a helpful assistant");
let user_event = Event::user("What is Rust?");
let assistant_event = Event::assistant("Rust is a systems programming language...");

// Access event properties
match event.role() {
    Role::System => println!("System message"),
    Role::User => println!("User message"),
    Role::Assistant => println!("Assistant message"),
    Role::Tool => println!("Tool response"),
}

let content = event.content();
println!("Message: {}", content.first_text().unwrap_or(""));
```

---

## LlmFunction - Simple Structured Outputs

`LlmFunction<T>` is perfect for when you want structured, typed responses without tool execution.

### Basic Usage

```rust
use radkit::agent::LlmFunction;
use radkit::models::providers::AnthropicLlm;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct MovieRecommendation {
    title: String,
    year: u16,
    genre: String,
    rating: f32,
    reason: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let llm = AnthropicLlm::from_env("claude-sonnet-4-5-20250929")?;
    let movie_fn = LlmFunction::<MovieRecommendation>::new(llm);

    let recommendation = movie_fn
        .run("Recommend a sci-fi movie for someone who loves The Matrix")
        .await?;

    println!("🎬 {}", recommendation.title);
    println!("📅 Year: {}", recommendation.year);
    println!("🎭 Genre: {}", recommendation.genre);
    println!("⭐ Rating: {}/10", recommendation.rating);
    println!("💡 {}", recommendation.reason);

    Ok(())
}
```

---

## LlmWorker - Tool Execution

`LlmWorker<T>` adds automatic tool calling and multi-turn execution loops to `LlmFunction`.

```rust
use radkit::agent::LlmWorker;
use radkit::models::providers::AnthropicLlm;
use radkit::tools::{FunctionTool, ToolResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WeatherReport {
    location: String,
    temperature: f64,
    condition: String,
    forecast: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create weather tool
    let weather_tool = Arc::new(FunctionTool::new(
        "get_weather",
        "Get current weather for a location",
        |args, _ctx| {
            Box::pin(async move {
                let location = args
                    .get("location")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown");

                // In real app, call weather API here
                let weather_data = json!({
                    "temperature": 72.5,
                    "condition": "Sunny",
                    "humidity": 65,
                });

                ToolResult::success(weather_data)
            })
        },
    ).with_parameters_schema(json!({
        "type": "object",
        "properties": {
            "location": {
                "type": "string",
                "description": "City name or location"
            }
        },
        "required": ["location"]
    })));

    // Create worker with tool
    let llm = AnthropicLlm::from_env("claude-sonnet-4-5-20250929")?;
    let worker = LlmWorker::<WeatherReport>::builder(llm)
        .with_system_instructions("You are a weather assistant")
        .with_tool(weather_tool)
        .build();

    // Run - LLM will automatically call the weather tool
    let report = worker.run("What's the weather in San Francisco?").await?;

    println!("📍 Location: {}", report.location);
    println!("🌡️  Temperature: {}°F", report.temperature);
    println!("☀️  Condition: {}", report.condition);
    println!("📅 {}", report.forecast);

    Ok(())
}
```