---
title: Composing Agents
description: Combine skills into unified agents and run them as A2A-compliant servers.
---



An agent is more than just a single skill; it's a collection of skills unified under a single identity and purpose. Radkit makes it easy to compose an agent from the skills you've built and run it as an A2A-compliant server.

## Composing an Agent

You compose an agent using the `Agent::builder()`. You give the agent an ID and name, and then you add your skills to it.

```rust
use radkit::agent::{Agent, AgentDefinition, OnRequestResult, SkillHandler};
use radkit::errors::AgentResult;
use radkit::models::Content;
use radkit::runtime::context::{Context, TaskContext};
use radkit::runtime::Runtime;

// Assume ProfileExtractorSkill and ReportGeneratorSkill are defined as in previous guides
# pub struct ProfileExtractorSkill;
# #[async_trait]
# impl SkillHandler for ProfileExtractorSkill {
#     async fn on_request(&self, _: &mut TaskContext, _: &Context, _: &dyn Runtime, _: Content) -> AgentResult<OnRequestResult> {
#         unimplemented!()
#     }
# }
# pub struct ReportGeneratorSkill;
# #[async_trait]
# impl SkillHandler for ReportGeneratorSkill {
#     async fn on_request(&self, _: &mut TaskContext, _: &Context, _: &dyn Runtime, _: Content) -> AgentResult<OnRequestResult> {
#         unimplemented!()
#     }
# }


// This function defines your agents.
pub fn configure_agents() -> Vec<AgentDefinition> {
    let my_agent = Agent::builder()
        .with_id("my-hr-agent-v1")
        .with_name("HR Assistant Agent")
        .with_description("An intelligent agent for handling HR tasks like resume processing and report generation.")
        // Add the skills to the agent
        .with_skill(ProfileExtractorSkill)
        .with_skill(ReportGeneratorSkill)
        .build();

    // You can define and return multiple agents from the same project
    vec![my_agent]
}
```

The `Agent::builder()` creates a serializable `AgentDefinition`. This definition is the blueprint for your agent that gets deployed.

## Running the Agent Locally

To test your agent, you can run it locally. The `DefaultRuntime` provides a simple, A2A-compliant web server for this purpose.

To enable the server, you must enable the `runtime` feature for Radkit in your `Cargo.toml`:

```toml
[dependencies]
radkit = { version = "0.0.2", features = ["runtime"] }
# ... other dependencies
```

Then, you can add a `main` function to run the server.

```rust
# use radkit::agent::AgentDefinition;
# use radkit::models::providers::AnthropicLlm;
# use radkit::runtime::DefaultRuntime;
# pub fn configure_agents() -> Vec<AgentDefinition> { vec![] }
// This main function will only be compiled for native targets, not for WASM.
#[cfg(not(all(target_os = "wasi", target_env = "p1")))]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Create an LLM instance
    let llm = AnthropicLlm::from_env("claude-sonnet-4-5-20250929")?;

    // 2. Create a default runtime environment with the LLM
    let runtime = DefaultRuntime::new(llm);

    // 3. Add the agents and start the server
    runtime
        .agents(configure_agents())
        .serve("127.0.0.1:8080")
        .await?;

    Ok(())
}
```

Now, run your project:

```bash
cargo run
```

Your A2A agent is now running at `http://127.0.0.1:8080`! You can interact with it using any A2A-compliant client.

### Optional Dev UI

Turn on the `dev-ui` feature for an interactive, browser-based playground that sits on top of the runtime:
