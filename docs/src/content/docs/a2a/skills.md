---
title: Skills
description: Building self-contained agent capabilities with Radkit's skill system.
---



In Radkit, the fundamental unit of capability for an A2A agent is the **Skill**. A skill is a self-contained piece of logic that can handle a specific type of task. An agent is essentially a collection of one or more skills.

## Defining a Skill

You define a skill by creating a struct and implementing the `SkillHandler` trait for it. You also annotate the struct with the `#[skill]` macro to provide the necessary A2A metadata.

### The `#[skill]` Macro

The `#[skill]` macro is how you tell Radkit about your skill's capabilities. This metadata is used to automatically generate the agent's "Agent Card," which other agents use for discovery.

```rust
use radkit::macros::skill;

#[skill(
    // A unique identifier for the skill
    id = "extract_profile",
    // A human-readable name
    name = "Profile Extractor",
    // A clear description of what the skill does
    description = "Extracts structured user profiles from text",
    // Tags for discovery
    tags = ["extraction", "profiles", "hr"],
    // Example prompts to show users
    examples = [
        "Extract a profile from: John Doe, john@example.com, Software Engineer",
        "Parse this resume into a profile"
    ],
    // The MIME types this skill can accept as input
    input_modes = ["text/plain", "application/pdf"],
    // The MIME types this skill can produce as output
    output_modes = ["application/json"]
)]
pub struct ProfileExtractorSkill;
```

### The `SkillHandler` Trait

The `SkillHandler` trait defines the logic for your skill. The only required method is `on_request`, which is the entry point for any new task assigned to the skill.

```rust
use radkit::agent::{Artifact, LlmFunction, OnRequestResult, SkillHandler};
use radkit::errors::AgentResult;
use radkit::macros::skill;
use radkit::models::Content;
use radkit::runtime::context::{Context, TaskContext};
use radkit::runtime::Runtime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// Define the output type for the skill
#[derive(Serialize, Deserialize, JsonSchema)]
struct UserProfile {
    name: String,
    email: String,
    role: String,
}

// The skill struct from before
#[skill(id = "extract_profile", name = "Profile Extractor", /* ... */)]
pub struct ProfileExtractorSkill;

// The implementation of the skill's logic
#[async_trait]
impl SkillHandler for ProfileExtractorSkill {
    async fn on_request(
        &self,
        task_context: &mut TaskContext,
        context: &Context,
        runtime: &dyn Runtime,
        content: Content,
    ) -> AgentResult<OnRequestResult> {
        // 1. Get the LLM from the runtime
        let llm = runtime.llm_provider().default_llm()?;

        // 2. Use an LlmFunction to perform the core logic
        let profile = LlmFunction::<UserProfile>::new(llm)
            .with_system_instructions("Extract the user's name, email, and role from the text.")
            .run(content.first_text().unwrap_or_default())
            .await?;

        // 3. Create an Artifact to return the structured data
        let artifact = Artifact::from_json("user_profile.json", &profile)?;

        // 4. Return a 'Completed' result
        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text("Profile extracted successfully.")),
            artifacts: vec![artifact],
        })
    }
}
```

### The `on_request` Handler

The `on_request` handler is the "happy path" for your skill. It receives the user's request (`content`) and has access to the `runtime` to get services like an LLM.

Its job is to attempt to complete the task. It can return one of several `OnRequestResult` variants, which map directly to A2A task states:

-   `OnRequestResult::Completed`: The skill successfully finished the task. This is the most common success case.
-   `OnRequestResult::InputRequired`: The skill needs more information from the user to continue. (See [Multi-turn Conversations](./multi-turn-conversations.md))
-   `OnRequestResult::Failed`: The skill encountered an unrecoverable error.
-   `OnRequestResult::Rejected`: The skill cannot or will not perform the requested task.

By returning one of these specific types, you are controlling the A2A task lifecycle, and Radkit ensures the correct protocol messages are sent automatically.
