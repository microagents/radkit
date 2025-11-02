// --- Skill: Summarize Resume (Attempt and Continue Pattern) ---

use radkit::agent::{
    Artifact, LlmFunction, LlmWorker, OnInputResult, OnRequestResult, SkillHandler, SkillSlot,
};
use radkit::errors::AgentError;
use radkit::models::providers::AnthropicLlm;
use radkit::models::{BaseLlm, Content};
use radkit::runtime::context::{Context, TaskContext};
use radkit::runtime::{MemoryServiceExt, Runtime};
use radkit::tools::{BaseTool, FunctionTool, ToolResult};
use radkit_macros::skill;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

// --- Data Structures, Enums, and Callable Helpers for this Skill ---

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
pub struct UserData {
    pub name: String,
    pub email: String,
    pub github_username: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
pub struct GitHubRepository {
    pub name: String,
    pub url: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FinalUser {
    pub name: String,
    pub email: String,
    pub github_username: String,
    pub repos: Vec<GitHubRepository>,
}

/// This enum defines all the possible pieces of information this skill might
/// need to ask the user for. It provides compile-time safety for input handling.
#[derive(Serialize, Deserialize)]
pub enum ResumeInputSlot {
    CorrectedUsername,
}

// Defines the callable struct for extracting user data.
fn extract_user_data() -> LlmFunction<UserData> {
    // Create LLM locally for this function
    let llm = AnthropicLlm::from_env("claude-sonnet-4-5-20250929")
        .expect("Failed to create LLM from environment");

    LlmFunction::new_with_system_instructions(
        llm,
        "Extract the user's name, email, and github username from the text.",
    )
}

// Defines the callable struct for extracting GitHub repos.
fn extract_user_github_repos() -> LlmWorker<Vec<GitHubRepository>> {
    let github_tool = Arc::new(github_api_tool()) as Arc<dyn BaseTool>;

    // Create LLM locally for this worker
    let llm = AnthropicLlm::from_env("claude-sonnet-4-5-20250929")
        .expect("Failed to create LLM from environment");

    LlmWorker::builder(llm)
        .with_system_instructions("Get the user's public GitHub repositories.")
        .with_tools(vec![github_tool])
        .build()
}

fn github_api_tool() -> FunctionTool {
    FunctionTool::new(
        "github_api",
        "Fetches public GitHub repositories for the requested username.",
        |mut args, _context| {
            let username = args
                .remove("github_username")
                .and_then(|value| value.as_str().map(|candidate| candidate.to_string()))
                .unwrap_or_default();

            #[cfg(not(all(target_os = "wasi", target_env = "p1")))]
            {
                Box::pin(async move {
                    let message = format!(
                        "GitHub API integration not implemented. Requested username: {}",
                        username
                    );
                    ToolResult::error(message)
                })
            }

            #[cfg(all(target_os = "wasi", target_env = "p1"))]
            {
                Box::pin(async move {
                    let message = format!(
                        "GitHub API not available in WASM sandbox. Requested username: {}",
                        username
                    );
                    ToolResult::error(message)
                })
            }
        },
    )
    .with_parameters_schema(json!({
        "type": "object",
        "properties": {
            "github_username": {
                "type": "string",
                "description": "GitHub handle to inspect for public repositories"
            }
        },
        "required": ["github_username"],
    }))
}

// --- Skill Logic Implementation ---

#[skill(
    id = "summarize_resume",
    name = "Resume Summarizer",
    description = "Extracts structured data from resumes and validates GitHub profiles",
    tags = ["hr", "resume", "onboarding"],
    examples = [
        "Summarize this resume: John Doe, john@example.com, github.com/johndoe",
        "Extract information from the attached resume"
    ],
    input_modes = ["text/plain", "application/pdf"],
    output_modes = ["application/json"]
)]
pub struct SummarizeResumeSkill;

#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl SkillHandler for SummarizeResumeSkill {
    /// The "Attempt" handler: tries the full happy path.
    async fn on_request(
        &self,
        task_context: &mut TaskContext,
        context: &Context,
        runtime: &dyn Runtime,
        content: Content,
    ) -> Result<OnRequestResult, AgentError> {
        // Send intermediate status update
        task_context
            .send_intermediate_update("Analyzing resume...")
            .await?;

        // Extract text from content
        let resume_text = content
            .first_text()
            .ok_or_else(|| AgentError::MissingInput("No text content in message".to_string()))?;

        // Extract user data
        let user_data = extract_user_data().run(resume_text).await?;

        runtime
            .memory()
            .save(&context.auth, "user_data", &user_data)
            .await?;

        // Send another intermediate update
        task_context
            .send_intermediate_update("Fetching GitHub repositories...")
            .await?;

        // Fetch GitHub repos
        let repos = extract_user_github_repos()
            .run(&user_data.github_username)
            .await?;

        if repos.is_empty() {
            // Happy path failed. Save partial data and request input.
            task_context.save_data("partial_user", &user_data)?;

            Ok(OnRequestResult::InputRequired {
                message: Content::from_text(
                    "No GitHub repos found. Please provide the correct username.",
                ),
                slot: SkillSlot::new(ResumeInputSlot::CorrectedUsername),
            })
        } else {
            // Happy path succeeded. Complete the task directly.
            let user = FinalUser {
                name: user_data.name,
                email: user_data.email,
                github_username: user_data.github_username,
                repos,
            };

            let artifact = Artifact::from_json("user_profile.json", &user)?;

            Ok(OnRequestResult::Completed {
                message: Some(Content::from_text(
                    "Successfully extracted user profile and repositories.",
                )),
                artifacts: vec![artifact],
            })
        }
    }

    /// The "Continue" handler: runs after user provides the missing input.
    async fn on_input_received(
        &self,
        task_context: &mut TaskContext,
        _context: &Context,
        runtime: &dyn Runtime,
        content: Content,
    ) -> Result<OnInputResult, AgentError> {
        let slot: ResumeInputSlot = task_context.load_slot()?.unwrap();

        match slot {
            ResumeInputSlot::CorrectedUsername => {
                let partial_user: UserData =
                    task_context.load_data("partial_user")?.ok_or_else(|| {
                        AgentError::ContextError("No partial user data found".to_string())
                    })?;

                let corrected_username = content
                    .first_text()
                    .ok_or_else(|| AgentError::MissingInput("No username provided".to_string()))?;

                let repos = extract_user_github_repos().run(corrected_username).await?;

                if repos.is_empty() {
                    Ok(OnInputResult::Failed {
                        error: Content::from_text(
                            "Could not find repositories with the provided username.",
                        ),
                    })
                } else {
                    let user = FinalUser {
                        name: partial_user.name,
                        email: partial_user.email,
                        github_username: corrected_username.to_string(),
                        repos,
                    };

                    let artifact = Artifact::from_json("user_profile.json", &user)?;

                    Ok(OnInputResult::Completed {
                        message: Some(Content::from_text(
                            "Successfully extracted user profile with corrected username.",
                        )),
                        artifacts: vec![artifact],
                    })
                }
            }
        }
    }
}
