// --- Skill: Create IT Accounts ---

use anyhow::Result;
use async_trait::async_trait;
use radkit::prelude::*;
use radkit::tools::A2AAgentTool;
use std::sync::Arc;

// --- Callable Helper for this Skill ---

fn ask_devops_to_create_accounts() -> LlmWorker<Task> {
    // Create A2A Agent Tool for delegating to DevOps agent
    let devops_tool = Arc::new(
        A2AAgentTool::new(
            "https://devops.example.com/.well-known/agent-card",
            "DevOps Agent for creating IT accounts",
        )
        .expect("Failed to create A2A Agent Tool"),
    ) as Arc<dyn BaseTool>;

    LlmWorker::builder()
        .with_system_instructions(
            "You are coordinating with the DevOps agent to create IT accounts. \
             Provide the role and name to the DevOps agent tool.",
        )
        .with_tools(vec![devops_tool])
        .build()
}

// --- Skill Logic Implementation ---

#[skill(
    id = "create_it_accounts",
    name = "IT Account Creator",
    description = "Creates IT accounts for new hires by delegating to the DevOps agent",
    tags = ["hr", "it", "accounts", "devops"],
    examples = [
        "Create IT accounts for John Doe as Software Engineer",
        "Set up accounts for the new hire"
    ],
    input_modes = ["text/plain", "application/json"],
    output_modes = ["application/json"]
)]
pub struct CreateItAccountsSkill;

#[async_trait]
impl SkillHandler for CreateItAccountsSkill {
    async fn on_request(
        &self,
        task_context: &mut TaskContext,
        _context: &Context,
        runtime: &dyn Runtime,
        content: Content,
    ) -> Result<OnRequestResult> {
        // Extract role and name from content
        // In a real implementation, this could be structured data
        let text = content
            .first_text()
            .ok_or_else(|| anyhow!("No text content in message"))?;

        // For this example, we'll use placeholder values
        let role = "Software Engineer";
        let name = "New Hire";

        // Send intermediate update
        task_context
            .send_intermediate_update(format!(
                "Requesting DevOps agent to create accounts for {} as {}...",
                name, role
            ))
            .await?;

        // Get LLM from runtime
        let llm = runtime.llm_provider().default_llm()?;

        // Call DevOps agent via LLM Worker with A2A tool
        let remote_task = ask_devops_to_create_accounts()
            .with_llm(llm)
            .run(&format!(
                "Create IT accounts for {} with role {}",
                name, role
            ))
            .await?;

        // Check the remote task result
        match remote_task.status.state {
            TaskState::Completed => Ok(OnRequestResult::Completed {
                message: Some(Content::from_text(
                    "DevOps agent successfully created all necessary accounts.",
                )),
                artifacts: remote_task.artifacts.unwrap_or_default(),
            }),
            TaskState::Failed => {
                let error_message = remote_task
                    .status
                    .message
                    .as_ref()
                    .and_then(|m| m.get_text())
                    .unwrap_or("Unknown error");

                Ok(OnRequestResult::Failed {
                    error: format!("DevOps agent failed to create accounts: {}", error_message),
                })
            }
            _ => Ok(OnRequestResult::Failed {
                error: format!(
                    "Remote DevOps agent finished in an unexpected state: {:?}",
                    remote_task.status.state
                ),
            }),
        }
    }
}
