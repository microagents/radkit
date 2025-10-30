# Developer Experience

This document outlines the intended developer experience, from creating a new agent to running it locally. The goal is to provide a workflow that is simple, powerful, and productive.

## 1. Creating a New Agent

A developer starts by defining their agent's logic and composition.

### Step 1: Define a Skill

The developer defines a skill using the `#[skill]` macro for A2A metadata, then implements the `SkillHandler` trait. For complex skills, they can override optional methods like `on_input_received`.

```rust
// In `skills/summarize_resume.rs`
use radkit::prelude::*;

// 1. Define the skill struct with A2A metadata.
#[skill(
    id = "summarize_resume",
    name = "Resume Summarizer",
    description = "Extracts user data from resumes",
    tags = ["hr", "resume"],
    examples = ["Summarize this resume: John Doe..."],
    input_modes = ["text/plain"],
    output_modes = ["application/json"]
)]
pub struct SummarizeResumeSkill;

// 2. Implement the SkillHandler trait.
#[async_trait]
impl SkillHandler for SummarizeResumeSkill {

    async fn on_request(
        &self,
        task_context: &mut TaskContext,
        context: &Context,
        runtime: &dyn Runtime,
        content: Content,
    ) -> AgentResult<OnRequestResult> {
        // Get LLM from runtime
        let llm = runtime.llm_provider().default_llm()?;

        // Send intermediate updates if needed
        task_context.send_intermediate_update("Analyzing resume...").await?;

        // Use LLM functions
        let user_data = extract_user_data()
            .with_llm(llm)
            .run(content.first_text().unwrap())
            .await?;

        // Attempt the happy path...
        if user_data.github_username.is_empty() {
            task_context.save_data("partial_user", &user_data)?;
            return Ok(OnRequestResult::InputRequired {
                message: Content::from_text("Please provide GitHub username"),
                slot: SkillSlot::new(ResumeInputSlot::Username),
            });
        }

        let artifact = Artifact::from_json("user_profile.json", &user_data)?;
        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text("Profile extracted!")),
            artifacts: vec![artifact],
        })
    }

    async fn on_input_received(
        &self,
        task_context: &mut TaskContext,
        context: &Context,
        runtime: &dyn Runtime,
        content: Content,
    ) -> AgentResult<OnInputResult> {
        // Continue the logic with the new user input...
        let partial: UserData = task_context.load_data("partial_user")?.unwrap();
        // ... process input ...
        Ok(OnInputResult::Completed {
            message: Some(Content::from_text("Done!")),
            artifacts: vec![/* ... */],
        })
    }
}
```

### Step 2: Compose the Agent

Next, the developer composes the final agent in their `main` function. They pass an *instance* of the skill struct to the `.with_skill()` method.

```rust
// In `agent.rs`

fn main() {
    let llm = AnthropicLlm::from_env("claude-sonnet-4").unwrap();
    let runtime = DefaultRuntime::new(llm);

    Agent::builder(
        "You are an HR agent dispatcher..."
    )
    .with_id("hr-agent-v1")
    .with_name("HR Agent")
    .with_skill(SummarizeResumeSkill)
    .with_skill(GenerateOnboardingPlanSkill)
    .with_runtime(runtime)
    .serve();
}
```

## 2. Running Locally

The `.serve()` method on the builder starts a local A2A-compliant web server, allowing the developer to immediately test their agent.

## 3. Upgrading to the Cloud

When ready for production, the developer can switch to the `PaidRuntime` and use the deployment CLI. (More on this later)
