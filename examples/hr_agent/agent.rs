// --- Skill Implementations ---
// The modular skills are defined in their own files.
mod hr_agent_skills;
use hr_agent_skills::create_it_accounts::CreateItAccountsSkill;
use hr_agent_skills::generate_onboarding_plan::GenerateOnboardingPlanSkill;
use hr_agent_skills::summarize_resume::SummarizeResumeSkill;

pub fn configure_agents() -> Vec<AgentDefinition> {
    let hr_agent = Agent::builder()
        .with_id("hr-agent-v1")
        .with_name("HR Agent")
        .with_description(
            "An intelligent HR agent that handles resume processing, onboarding, \
             and IT account creation. Supports multi-turn conversations and \
             delegates to specialized agents when needed.",
        )
        // Skills are automatically discovered via #[skill] macro metadata
        .with_skill(SummarizeResumeSkill)
        .with_skill(GenerateOnboardingPlanSkill)
        .with_skill(CreateItAccountsSkill)
        .build();

    vec![hr_agent]
}

/// The main entry point for local, native development and testing.
/// This function is excluded from WASM builds via the `#[cfg]` attribute.
#[cfg(not(all(target_os = "wasi", target_env = "p1")))]
fn main() {
    radkit::runtime::block_on(async {
        // For local testing, the developer chooses their desired runtime.
        let runtime = DefaultRuntime::new();

        // The `serve` method on the local runtime takes the agent definitions
        // and starts a local A2A-compliant web server.
        // Agent metadata is automatically extracted from #[skill] macros
        // and exposed via the agent card endpoint.
        runtime
            .with_agents(configure_agents())
            .serve("127.0.0.1:8080")
            .await
            .expect("Failed to start server");
    });
}

/// WASM builds compile this example as a module and expose no native entrypoint.
/// The cloud platform will call the #[entrypoint] function to get agent definitions.
#[cfg(all(target_os = "wasi", target_env = "p1"))]
fn main() {}
