use radkit::agent::{
    Agent, AgentDefinition, Artifact, OnInputResult, OnRequestResult, RegisteredSkill,
    SkillHandler, SkillMetadata, SkillSlot,
};
use radkit::errors::AgentError;
use radkit::models::Content;
use radkit::runtime::context::{Context, TaskContext};
use radkit::runtime::{AgentRuntime, Runtime};
use radkit::test_support::FakeLlm;

// A test skill for verifying lifecycle behavior.
struct LifecycleSkill;

static LIFECYCLE_METADATA: SkillMetadata = SkillMetadata::new(
    "lifecycle_skill",
    "Lifecycle Skill",
    "A skill for testing the execution lifecycle.",
    &[],
    &[],
    &[],
    &[],
);

#[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
enum LifecycleSlot {
    AwaitingInput,
}

#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl SkillHandler for LifecycleSkill {
    async fn on_request(
        &self,
        task_context: &mut TaskContext,
        _context: &Context,
        _runtime: &dyn AgentRuntime,
        _content: Content,
    ) -> Result<OnRequestResult, AgentError> {
        task_context.save_data("request_seen", &true)?;
        Ok(OnRequestResult::InputRequired {
            message: Content::from_text("Please provide input."),
            slot: SkillSlot::new(LifecycleSlot::AwaitingInput),
        })
    }

    async fn on_input_received(
        &self,
        task_context: &mut TaskContext,
        _context: &Context,
        _runtime: &dyn AgentRuntime,
        content: Content,
    ) -> Result<OnInputResult, AgentError> {
        let request_seen: bool = task_context.load_data("request_seen")?.unwrap_or(false);
        assert!(request_seen, "on_request should have been called first");

        let slot: LifecycleSlot = task_context.load_slot()?.expect("slot should be set");
        assert_eq!(slot, LifecycleSlot::AwaitingInput);

        let input_text = content.first_text().unwrap_or("");
        if input_text == "complete" {
            Ok(OnInputResult::Completed {
                message: Some(Content::from_text("Completed!")),
                artifacts: vec![],
            })
        } else {
            Ok(OnInputResult::Failed {
                error: Content::from_text("Invalid input"),
            })
        }
    }
}

impl RegisteredSkill for LifecycleSkill {
    fn metadata() -> &'static SkillMetadata {
        &LIFECYCLE_METADATA
    }
}

fn lifecycle_agent_definition() -> AgentDefinition {
    Agent::builder()
        .with_id("test-agent")
        .with_skill(LifecycleSkill)
        .build()
}

#[tokio::test]
async fn test_skill_lifecycle() {
    let llm = FakeLlm::with_responses("fake_llm", std::iter::empty());
    let agent = lifecycle_agent_definition();
    let runtime = Runtime::builder(lifecycle_agent_definition(), llm).build();

    let skill = agent.skills().first().unwrap();
    let mut task_context = TaskContext::new();
    let auth_context = runtime.auth().get_auth_context();
    let context = Context::new(auth_context);

    // 1. Test on_request
    let request_result = skill
        .handler()
        .on_request(
            &mut task_context,
            &context,
            &runtime,
            Content::from_text("start"),
        )
        .await
        .unwrap();

    match request_result {
        OnRequestResult::InputRequired { message, slot } => {
            assert_eq!(message.first_text(), Some("Please provide input."));
            let slot_value: LifecycleSlot = slot.deserialize().unwrap();
            assert_eq!(slot_value, LifecycleSlot::AwaitingInput);
            // Simulate what the executor does: store the slot for continuation
            task_context.set_pending_slot(slot);
        }
        _ => panic!("Expected InputRequired"),
    }

    // 2. Test on_input_received
    let input_result = skill
        .handler()
        .on_input_received(
            &mut task_context,
            &context,
            &runtime,
            Content::from_text("complete"),
        )
        .await
        .unwrap();

    match input_result {
        OnInputResult::Completed { message, .. } => {
            assert_eq!(message.unwrap().first_text(), Some("Completed!"));
        }
        _ => panic!("Expected Completed"),
    }
}

// Test skill that returns Failed on invalid input
#[tokio::test]
async fn test_skill_lifecycle_with_failure() {
    let llm = FakeLlm::with_responses("fake_llm", std::iter::empty());
    let agent = lifecycle_agent_definition();
    let runtime = Runtime::builder(lifecycle_agent_definition(), llm).build();

    let skill = agent.skills().first().unwrap();
    let mut task_context = TaskContext::new();
    let auth_context = runtime.auth().get_auth_context();
    let context = Context::new(auth_context);

    // 1. Call on_request
    let request_result = skill
        .handler()
        .on_request(
            &mut task_context,
            &context,
            &runtime,
            Content::from_text("start"),
        )
        .await
        .unwrap();

    // 2. Extract and set the slot
    if let OnRequestResult::InputRequired { slot, .. } = request_result {
        task_context.set_pending_slot(slot);
    }

    // 3. Test on_input_received with invalid input
    let input_result = skill
        .handler()
        .on_input_received(
            &mut task_context,
            &context,
            &runtime,
            Content::from_text("invalid"),
        )
        .await
        .unwrap();

    match input_result {
        OnInputResult::Failed { error } => {
            assert_eq!(error.first_text(), Some("Invalid input"));
        }
        _ => panic!("Expected Failed"),
    }
}

// Test skill that completes immediately without requiring input
struct ImmediateSkill;

static IMMEDIATE_METADATA: SkillMetadata = SkillMetadata::new(
    "immediate_skill",
    "Immediate Skill",
    "A skill that completes immediately.",
    &[],
    &[],
    &[],
    &[],
);

#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl SkillHandler for ImmediateSkill {
    async fn on_request(
        &self,
        _task_context: &mut TaskContext,
        _context: &Context,
        _runtime: &dyn AgentRuntime,
        content: Content,
    ) -> Result<OnRequestResult, AgentError> {
        let text = content.first_text().unwrap_or("no input");
        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text(format!("Processed: {text}"))),
            artifacts: vec![Artifact::from_text("result", "success")],
        })
    }

    async fn on_input_received(
        &self,
        _task_context: &mut TaskContext,
        _context: &Context,
        _runtime: &dyn AgentRuntime,
        _content: Content,
    ) -> Result<OnInputResult, AgentError> {
        panic!("on_input_received should not be called for immediate completion");
    }
}

impl RegisteredSkill for ImmediateSkill {
    fn metadata() -> &'static SkillMetadata {
        &IMMEDIATE_METADATA
    }
}

fn immediate_agent_definition() -> AgentDefinition {
    Agent::builder()
        .with_id("test-agent")
        .with_skill(ImmediateSkill)
        .build()
}

#[tokio::test]
async fn test_immediate_completion_skill() {
    let llm = FakeLlm::with_responses("fake_llm", std::iter::empty());
    let agent = immediate_agent_definition();
    let runtime = Runtime::builder(immediate_agent_definition(), llm).build();

    let skill = agent.skills().first().unwrap();
    let mut task_context = TaskContext::new();
    let auth_context = runtime.auth().get_auth_context();
    let context = Context::new(auth_context);

    let request_result = skill
        .handler()
        .on_request(
            &mut task_context,
            &context,
            &runtime,
            Content::from_text("test"),
        )
        .await
        .unwrap();

    match request_result {
        OnRequestResult::Completed { message, artifacts } => {
            assert_eq!(message.unwrap().first_text(), Some("Processed: test"));
            assert_eq!(artifacts.len(), 1);
            assert_eq!(artifacts[0].name(), "result");
        }
        _ => panic!("Expected Completed"),
    }
}

// Test skill that rejects requests
struct RejectingSkill;

static REJECTING_METADATA: SkillMetadata = SkillMetadata::new(
    "rejecting_skill",
    "Rejecting Skill",
    "A skill that rejects certain requests.",
    &[],
    &[],
    &[],
    &[],
);

#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl SkillHandler for RejectingSkill {
    async fn on_request(
        &self,
        _task_context: &mut TaskContext,
        _context: &Context,
        _runtime: &dyn AgentRuntime,
        content: Content,
    ) -> Result<OnRequestResult, AgentError> {
        let text = content.first_text().unwrap_or("");
        if text.contains("forbidden") {
            Ok(OnRequestResult::Rejected {
                reason: Content::from_text("This request is forbidden"),
            })
        } else {
            Ok(OnRequestResult::Completed {
                message: Some(Content::from_text("Accepted")),
                artifacts: vec![],
            })
        }
    }

    async fn on_input_received(
        &self,
        _task_context: &mut TaskContext,
        _context: &Context,
        _runtime: &dyn AgentRuntime,
        _content: Content,
    ) -> Result<OnInputResult, AgentError> {
        panic!("on_input_received should not be called for rejected requests");
    }
}

impl RegisteredSkill for RejectingSkill {
    fn metadata() -> &'static SkillMetadata {
        &REJECTING_METADATA
    }
}

fn rejecting_agent_definition() -> AgentDefinition {
    Agent::builder()
        .with_id("test-agent")
        .with_skill(RejectingSkill)
        .build()
}

#[tokio::test]
async fn test_rejecting_skill() {
    let llm = FakeLlm::with_responses("fake_llm", std::iter::empty());
    let agent = rejecting_agent_definition();
    let runtime = Runtime::builder(rejecting_agent_definition(), llm).build();

    let skill = agent.skills().first().unwrap();
    let mut task_context = TaskContext::new();
    let auth_context = runtime.auth().get_auth_context();
    let context = Context::new(auth_context);

    // Test rejection
    let request_result = skill
        .handler()
        .on_request(
            &mut task_context,
            &context,
            &runtime,
            Content::from_text("forbidden action"),
        )
        .await
        .unwrap();

    match request_result {
        OnRequestResult::Rejected { reason } => {
            assert_eq!(reason.first_text(), Some("This request is forbidden"));
        }
        _ => panic!("Expected Rejected"),
    }

    // Test acceptance
    let mut task_context = TaskContext::new();
    let request_result = skill
        .handler()
        .on_request(
            &mut task_context,
            &context,
            &runtime,
            Content::from_text("allowed action"),
        )
        .await
        .unwrap();

    match request_result {
        OnRequestResult::Completed { .. } => {
            // Success
        }
        _ => panic!("Expected Completed"),
    }
}

// Test skill with multi-round input requests
struct MultiRoundSkill;

static MULTI_ROUND_METADATA: SkillMetadata = SkillMetadata::new(
    "multi_round_skill",
    "Multi-Round Skill",
    "A skill that requires multiple rounds of input.",
    &[],
    &[],
    &[],
    &[],
);

#[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
enum MultiRoundSlot {
    AwaitingName,
    AwaitingAge { name: String },
}

#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl SkillHandler for MultiRoundSkill {
    async fn on_request(
        &self,
        _task_context: &mut TaskContext,
        _context: &Context,
        _runtime: &dyn AgentRuntime,
        _content: Content,
    ) -> Result<OnRequestResult, AgentError> {
        Ok(OnRequestResult::InputRequired {
            message: Content::from_text("What is your name?"),
            slot: SkillSlot::new(MultiRoundSlot::AwaitingName),
        })
    }

    async fn on_input_received(
        &self,
        task_context: &mut TaskContext,
        _context: &Context,
        _runtime: &dyn AgentRuntime,
        content: Content,
    ) -> Result<OnInputResult, AgentError> {
        let slot: MultiRoundSlot = task_context.load_slot()?.expect("slot should be set");

        match slot {
            MultiRoundSlot::AwaitingName => {
                let name = content.first_text().unwrap_or("Unknown").to_string();
                Ok(OnInputResult::InputRequired {
                    message: Content::from_text("What is your age?"),
                    slot: SkillSlot::new(MultiRoundSlot::AwaitingAge { name }),
                })
            }
            MultiRoundSlot::AwaitingAge { name } => {
                let age = content.first_text().unwrap_or("0");
                Ok(OnInputResult::Completed {
                    message: Some(Content::from_text(format!("Hello {name}, age {age}!"))),
                    artifacts: vec![],
                })
            }
        }
    }
}

impl RegisteredSkill for MultiRoundSkill {
    fn metadata() -> &'static SkillMetadata {
        &MULTI_ROUND_METADATA
    }
}

fn multi_round_agent_definition() -> AgentDefinition {
    Agent::builder()
        .with_id("test-agent")
        .with_skill(MultiRoundSkill)
        .build()
}

#[tokio::test]
async fn test_multi_round_input_skill() {
    let llm = FakeLlm::with_responses("fake_llm", std::iter::empty());
    let agent = multi_round_agent_definition();
    let runtime = Runtime::builder(multi_round_agent_definition(), llm).build();

    let skill = agent.skills().first().unwrap();
    let mut task_context = TaskContext::new();
    let auth_context = runtime.auth().get_auth_context();
    let context = Context::new(auth_context);

    // Round 1: Initial request
    let request_result = skill
        .handler()
        .on_request(
            &mut task_context,
            &context,
            &runtime,
            Content::from_text("start"),
        )
        .await
        .unwrap();

    match request_result {
        OnRequestResult::InputRequired { message, slot } => {
            assert_eq!(message.first_text(), Some("What is your name?"));
            task_context.set_pending_slot(slot);
        }
        _ => panic!("Expected InputRequired"),
    }

    // Round 2: Provide name
    let input_result = skill
        .handler()
        .on_input_received(
            &mut task_context,
            &context,
            &runtime,
            Content::from_text("Alice"),
        )
        .await
        .unwrap();

    match input_result {
        OnInputResult::InputRequired { message, slot } => {
            assert_eq!(message.first_text(), Some("What is your age?"));
            task_context.set_pending_slot(slot);
        }
        _ => panic!("Expected InputRequired"),
    }

    // Round 3: Provide age
    let input_result = skill
        .handler()
        .on_input_received(
            &mut task_context,
            &context,
            &runtime,
            Content::from_text("30"),
        )
        .await
        .unwrap();

    match input_result {
        OnInputResult::Completed { message, .. } => {
            assert_eq!(message.unwrap().first_text(), Some("Hello Alice, age 30!"));
        }
        _ => panic!("Expected Completed"),
    }
}
