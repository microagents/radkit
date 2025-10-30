// --- Skill: Generate Onboarding Plan ---

use super::summarize_resume::UserData;
use anyhow::Result;
use radkit::prelude::*;

fn generate_onboarding_tasks() -> LlmFunction<Vec<String>> {
    LlmFunction::new_with_system_instructions(
        "Generate a comprehensive list of onboarding tasks for the provided role. \
         Include technical setup, documentation review, and team introductions.",
    )
}

#[skill(
    id = "generate_onboarding_plan",
    name = "Onboarding Plan Generator",
    description = "Generates personalized onboarding plans for new hires",
    tags = ["hr", "onboarding", "planning"],
    examples = [
        "Generate an onboarding plan for a Software Engineer",
        "Create onboarding tasks for the new hire"
    ],
    input_modes = ["text/plain", "application/json"],
    output_modes = ["application/json"]
)]
pub struct GenerateOnboardingPlanSkill;

#[async_trait]
impl SkillHandler for GenerateOnboardingPlanSkill {
    async fn on_request(
        &self,
        task_context: &mut TaskContext,
        context: &Context,
        runtime: &dyn Runtime,
        _content: Content,
    ) -> Result<OnRequestResult> {
        // Send intermediate update
        task_context
            .send_intermediate_update("Looking for user profile...")
            .await?;

        let tasks = context.get_tasks()?;
        let user_data_artifact = tasks.iter().find_map(|task| {
            task.artifacts
                .as_ref()?
                .iter()
                .find(|art| art.name == Some("user_profile.json".to_string()))
        });

        if let Some(artifact) = user_data_artifact {
            let user_data: UserData = artifact.get_data()?;
            let role = "Software Engineer"; // Placeholder - could be extracted from context

            // Send intermediate update
            task_context
                .send_intermediate_update(format!(
                    "Generating onboarding tasks for {} role...",
                    role
                ))
                .await?;

            // Get LLM from runtime
            let llm = runtime.llm_provider().default_llm()?;

            let tasks = generate_onboarding_tasks().with_llm(llm).run(&role).await?;

            let plan_artifact = Artifact::from_json("onboarding_plan.json", &tasks)?;

            Ok(OnRequestResult::Completed {
                message: Some(Content::from_text(format!(
                    "Onboarding plan generated for {}.",
                    user_data.name
                ))),
                artifacts: vec![plan_artifact],
            })
        } else {
            Ok(OnRequestResult::Failed {
                error: "Could not find a summarized resume in the current session.".to_string(),
            })
        }
    }
}
