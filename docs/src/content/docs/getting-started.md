---
title: Getting Started
description: Learn how to install Radkit and build your first LLM-powered application with structured outputs.
---

Welcome to Radkit! This guide will walk you through installing the library and building your first simple application: a structured data extractor powered by an LLM.

## 1. Installation

First, you need to add `radkit` and a few other essential crates to your `Cargo.toml`.

```toml
[dependencies]
radkit = "0.0.3"
tokio = { version = "1", features = ["rt-multi-thread", "sync", "net", "process", "macros"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "1"
```

-   **radkit**: The main library.
-   **tokio**: For the asynchronous runtime (the listed features match what Radkit needs for native builds).
-   **serde**: For serializing and deserializing data.
-   **serde_json**: For working with JSON.
-   **schemars**: For generating JSON schemas from your Rust types.

## 2. Set up your Environment

This example uses the Anthropic Claude model. You'll need an API key from Anthropic.

Set it as an environment variable in your shell:

```bash
export ANTHROPIC_API_KEY="your-api-key-here"
```

:::tip
Radkit supports multiple LLM providers! You can also use `OPENAI_API_KEY`, `GEMINI_API_KEY`, etc., and the corresponding `Llm` struct.
:::

## 3. Create your First `LlmFunction`

Now, let's write some code. We'll create an `LlmFunction` that takes a user prompt and returns a structured `MovieRecommendation`.

Create a new file `main.rs`:

```rust
use radkit::agent::LlmFunction;
use radkit::models::providers::AnthropicLlm;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// 1. Define the structure of your desired output.
// The `JsonSchema` trait is required for structured output.
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
    // 2. Initialize the LLM provider.
    // This will automatically pick up the ANTHROPIC_API_KEY from the environment.
    let llm = AnthropicLlm::from_env("claude-3-sonnet-20240229")?;

    // 3. Create an LlmFunction for your target type.
    let movie_fn = LlmFunction::<MovieRecommendation>::new(llm);

    // 4. Run the function with a prompt!
    let recommendation = movie_fn
        .run("Recommend a sci-fi movie for someone who loves The Matrix")
        .await?;

    // 5. Use your structured, type-safe result.
    println!("🎬 {}", recommendation.title);
    println!("📅 Year: {}", recommendation.year);
    println!("🎭 Genre: {}", recommendation.genre);
    println!("⭐ Rating: {}/10", recommendation.rating);
    println!("💡 {}", recommendation.reason);

    Ok(())
}
```

## 4. Run it!

Execute the code with `cargo run`:

```bash
cargo run
```

You should see output like this (the movie will vary!):

```
🎬 Inception
📅 Year: 2010
🎭 Genre: Sci-Fi/Thriller
⭐ Rating: 8.8/10
💡 It's a mind-bending heist film that explores similar themes of reality and perception.
```

## Next Steps

Congratulations! You've successfully used `LlmFunction` to get structured data from an LLM.

-   Learn more about the building blocks of Radkit in **[Core Concepts](./core-concepts/index.md)**.
-   Explore how to make agents that can use tools in the **[Tool Execution](./guides/tool-execution.md)** guide.
