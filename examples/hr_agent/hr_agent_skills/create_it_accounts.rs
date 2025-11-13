// --- Skill: Create IT Accounts ---

use radkit::agent::{OnRequestResult, SkillHandler};
use radkit::errors::AgentError;
use radkit::macros::skill;
use radkit::models::Content;
use radkit::runtime::context::{Context, TaskContext};
use radkit::runtime::Runtime;

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

#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl SkillHandler for CreateItAccountsSkill {
    async fn on_request(
        &self,
        task_context: &mut TaskContext,
        _context: &Context,
        _runtime: &dyn Runtime,
        content: Content,
    ) -> Result<OnRequestResult, AgentError> {
        // Extract role and name from content
        let text = content
            .first_text()
            .ok_or_else(|| AgentError::MissingInput("No text content in message".to_string()))?;

        // For this example, we'll use placeholder values
        let role = "Software Engineer";
        let name = "New Hire";

        // Send intermediate update
        task_context
            .send_intermediate_update(format!("Creating IT accounts for {} as {}...", name, role))
            .await?;

        // TODO: In a real implementation, this would call the DevOps agent via A2A
        // For now, we'll just simulate success

        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text(
                "IT accounts created successfully (simulated).",
            )),
            artifacts: vec![],
        })
    }
}
