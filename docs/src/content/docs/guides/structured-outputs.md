---
title: Structured Outputs
description: Learn how to get structured, typed data from LLMs using LlmFunction.
---



While getting text from an LLM is useful, getting structured, typed data is a superpower. Radkit makes this easy with `LlmFunction<T>`.

`LlmFunction<T>` is a generic helper that instructs the LLM to respond with a JSON object matching the schema of your target type `T`. Radkit handles the prompting, schema generation, and deserialization, so you can work with type-safe Rust structs directly.

## Basic Usage

Let's revisit the example from the Getting Started guide.

```rust
use radkit::agent::LlmFunction;
use radkit::models::providers::AnthropicLlm;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// 1. Your target data structure
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
    let llm = AnthropicLlm::from_env("claude-3-sonnet-20240229")?;
    
    // 2. Create the LlmFunction
    let movie_fn = LlmFunction::<MovieRecommendation>::new(llm);

    // 3. Run it
    let recommendation = movie_fn
        .run("Recommend a sci-fi movie for someone who loves The Matrix")
        .await?;

    println!("🎬 {}", recommendation.title);
    Ok(())
}
```

### How it Works

1.  You define a Rust struct and derive `Serialize`, `Deserialize`, and `JsonSchema`.
2.  Radkit uses `schemars` to generate a JSON Schema for your struct at runtime.
3.  It injects a system prompt telling the LLM to respond with JSON matching that schema.
4.  It calls the LLM with your prompt.
5.  It parses the JSON response and deserializes it into an instance of your struct.

## System Instructions

You can provide system instructions to `LlmFunction` to give the LLM more context and guidance, improving the quality of the output.

```rust
use radkit::agent::LlmFunction;
use radkit::models::providers::AnthropicLlm;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct CodeReview {
    issues: Vec<String>,
    suggestions: Vec<String>,
    severity: String, // e.g., "Critical", "High", "Medium", "Low"
}

let llm = AnthropicLlm::from_env("claude-3-sonnet-20240229")?;

let review_fn = LlmFunction::<CodeReview>::new_with_system_instructions(
    llm,
    "You are a senior Rust code reviewer. Be thorough but constructive. Focus on idiomatic code, performance, and potential bugs."
);

let code = r#"#,
    fn divide(a: i32, b: i32) -> i32 {
        a / b
    }
"#;

let review = review_fn.run(format!("Review this code:\n{}", code)).await?;

println!("Severity: {}", review.severity);
for issue in review.issues {
    println!("- {}", issue);
}
```

## Multi-Turn Conversations

`LlmFunction` can also maintain conversation history. The `run_and_continue` method returns the result and the updated `Thread`, which you can use for follow-up questions.

```rust
use radkit::agent::LlmFunction;
use radkit::models::{providers::AnthropicLlm, Event};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct Answer {
    response: String,
    confidence: f32,
}

let llm = AnthropicLlm::from_env("claude-3-sonnet-20240229")?;
let qa_fn = LlmFunction::<Answer>::new(llm);

// First question
let (answer1, thread) = qa_fn
    .run_and_continue("What is Rust?")
    .await?;

println!("Q1: {} (Confidence: {})", answer1.response, answer1.confidence);

// Follow-up question (continues the conversation)
let (answer2, thread) = qa_fn
    .run_and_continue(
        thread.add_event(Event::user("What are its main benefits?"))
    )
    .await?;

println!("Q2: {}", answer2.response);
```

`LlmFunction` is the simplest way to add structured data capabilities to your agent.