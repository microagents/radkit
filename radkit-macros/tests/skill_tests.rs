use radkit::agent::{OnRequestResult, RegisteredSkill, SkillHandler};
use radkit::errors::AgentError;
use radkit::models::Content;
use radkit::runtime::context::{Context, TaskContext};
use radkit::runtime::Runtime;
use radkit_macros::skill;

// Test 1: Basic skill with required fields only
#[skill(id = "test_skill", name = "Test Skill", description = "A test skill")]
struct TestSkill;

#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    async_trait::async_trait(?Send)
)]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl SkillHandler for TestSkill {
    async fn on_request(
        &self,
        _task_context: &mut TaskContext,
        _context: &Context,
        _runtime: &dyn Runtime,
        _content: Content,
    ) -> Result<OnRequestResult, AgentError> {
        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text("Test response")),
            artifacts: vec![],
        })
    }
}

// Test 2: Skill with all fields
#[skill(
    id = "full_skill",
    name = "Full Test Skill",
    description = "A skill with all parameters",
    tags = ["test", "example"],
    examples = ["Example 1", "Example 2"],
    input_modes = ["text/plain", "application/json"],
    output_modes = ["text/plain"]
)]
struct FullSkill;

#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    async_trait::async_trait(?Send)
)]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl SkillHandler for FullSkill {
    async fn on_request(
        &self,
        _task_context: &mut TaskContext,
        _context: &Context,
        _runtime: &dyn Runtime,
        _content: Content,
    ) -> Result<OnRequestResult, AgentError> {
        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text("Full skill response")),
            artifacts: vec![],
        })
    }
}

// Test 3: Skill with empty arrays
#[skill(
    id = "empty_arrays_skill",
    name = "Empty Arrays Skill",
    description = "A skill with explicit empty arrays",
    tags = [],
    examples = [],
    input_modes = [],
    output_modes = []
)]
struct EmptyArraysSkill;

#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    async_trait::async_trait(?Send)
)]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl SkillHandler for EmptyArraysSkill {
    async fn on_request(
        &self,
        _task_context: &mut TaskContext,
        _context: &Context,
        _runtime: &dyn Runtime,
        _content: Content,
    ) -> Result<OnRequestResult, AgentError> {
        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text("Empty arrays skill response")),
            artifacts: vec![],
        })
    }
}

#[test]
fn test_basic_skill_metadata() {
    // Verify that metadata() is callable
    let metadata = TestSkill::metadata();
    assert_eq!(metadata.id, "test_skill");
    assert_eq!(metadata.name, "Test Skill");
    assert_eq!(metadata.description, "A test skill");
    assert_eq!(metadata.tags.len(), 0);
    assert_eq!(metadata.examples.len(), 0);
    assert_eq!(metadata.input_modes.len(), 0);
    assert_eq!(metadata.output_modes.len(), 0);
}

#[test]
fn test_full_skill_metadata() {
    let metadata = FullSkill::metadata();
    assert_eq!(metadata.id, "full_skill");
    assert_eq!(metadata.name, "Full Test Skill");
    assert_eq!(metadata.description, "A skill with all parameters");
    assert_eq!(metadata.tags, &["test", "example"]);
    assert_eq!(metadata.examples, &["Example 1", "Example 2"]);
    assert_eq!(metadata.input_modes, &["text/plain", "application/json"]);
    assert_eq!(metadata.output_modes, &["text/plain"]);
}

#[test]
fn test_empty_arrays_skill_metadata() {
    let metadata = EmptyArraysSkill::metadata();
    assert_eq!(metadata.id, "empty_arrays_skill");
    assert_eq!(metadata.name, "Empty Arrays Skill");
    assert_eq!(metadata.description, "A skill with explicit empty arrays");
    assert_eq!(metadata.tags.len(), 0);
    assert_eq!(metadata.examples.len(), 0);
    assert_eq!(metadata.input_modes.len(), 0);
    assert_eq!(metadata.output_modes.len(), 0);
}
