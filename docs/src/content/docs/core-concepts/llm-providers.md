---
title: LLM Providers
description: Working with multiple LLM providers including Anthropic, OpenAI, OpenRouter, Gemini, Grok, and DeepSeek.
---



Radkit supports multiple LLM providers through a unified `BaseLlm` trait. This allows you to switch between models from different providers with minimal code changes.

All providers follow a similar pattern:

1.  **Initialization**: Create an LLM client, usually from an environment variable containing the API key.
2.  **Configuration**: Optionally, chain builder methods to configure parameters like `max_tokens` or `temperature`.
3.  **Execution**: Call `generate_content` (or the simpler `generate` for text-only) with a `Thread`.

:::tip[API Keys]
It is best practice to load API keys from environment variables rather than hardcoding them in your source code.
-   Anthropic: `ANTHROPIC_API_KEY`
-   OpenAI: `OPENAI_API_KEY`
-   OpenRouter: `OPENROUTER_API_KEY`
-   Gemini: `GEMINI_API_KEY`
-   Grok: `XAI_API_KEY`
-   DeepSeek: `DEEPSEEK_API_KEY`
:::

## Anthropic (Claude)

```rust
use radkit::models::providers::AnthropicLlm;
use radkit::models::{BaseLlm, Thread};

// From environment variable (ANTHROPIC_API_KEY)
let llm = AnthropicLlm::from_env("claude-3-sonnet-20240229")?;

// With configuration
let llm = AnthropicLlm::from_env("claude-3-opus-20240229")?
    .with_max_tokens(4096)
    .with_temperature(0.7);

// Generate content
let thread = Thread::from_user("Explain quantum computing");
let response = llm.generate_content(thread, None).await?;

println!("Response: {}", response.content().first_text().unwrap());
```

## OpenAI (GPT)

```rust
use radkit::models::providers::OpenAILlm;
use radkit::models::BaseLlm;

// From environment variable (OPENAI_API_KEY)
let llm = OpenAILlm::from_env("gpt-4o")?;

// With configuration
let llm = OpenAILlm::from_env("gpt-4o-mini")?
    .with_max_tokens(2000)
    .with_temperature(0.5);

let response = llm.generate("What is machine learning?", None).await?;
```

## OpenRouter

OpenRouter is an OpenAI-compatible gateway that lets you route requests to many hosted models (Anthropic, Google, Cohere, etc.) via a single API surface.

```rust
use radkit::models::providers::OpenRouterLlm;
use radkit::models::BaseLlm;

// From environment variable (OPENROUTER_API_KEY)
let llm = OpenRouterLlm::from_env("anthropic/claude-3.5-sonnet")?
    .with_site_url("https://example.com")
    .with_app_name("My Radkit Agent");

let response = llm.generate("Summarize the latest release notes", None).await?;
```

## Google Gemini

```rust
use radkit::models::providers::GeminiLlm;
use radkit::models::BaseLlm;

// From environment variable (GEMINI_API_KEY)
let llm = GeminiLlm::from_env("gemini-1.5-flash-latest")?;

let response = llm.generate("Explain neural networks", None).await?;
```

## Grok (xAI)

```rust
use radkit::models::providers::GrokLlm;
use radkit::models::BaseLlm;

// From environment variable (XAI_API_KEY)
let llm = GrokLlm::from_env("grok-1.5-flash")?;

let response = llm.generate("What is the meaning of life?", None).await?;
```

## DeepSeek

```rust
use radkit::models::providers::DeepSeekLlm;
use radkit::models::BaseLlm;

// From environment variable (DEEPSEEK_API_KEY)
let llm = DeepSeekLlm::from_env("deepseek-chat")?;

let response = llm.generate("Code review best practices", None).await?;
```
