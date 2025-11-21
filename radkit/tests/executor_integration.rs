//! Integration tests for RequestExecutor orchestration.
//!
//! These tests verify that the RequestExecutor correctly coordinates skills,
//! task management, negotiation, and multi-turn conversations following the
//! A2A protocol.

#[cfg(all(
    feature = "runtime",
    feature = "test-support",
    not(all(target_os = "wasi", target_env = "p1"))
))]
mod tests {
    use a2a_types::{Message, MessageRole, MessageSendParams, Part, TaskQueryParams};
    use radkit::agent::{
        Agent, OnInputResult, OnRequestResult, RegisteredSkill, SkillHandler, SkillMetadata,
        SkillSlot,
    };
    use radkit::errors::AgentError;
    use radkit::models::{Content, LlmResponse, TokenUsage};
    use radkit::runtime::context::{Context, TaskContext};
    use radkit::runtime::core::executor::{ExecutorRuntime, RequestExecutor};
    use radkit::runtime::{AgentRuntime, Runtime};
    use radkit::test_support::FakeLlm;
    use serde::{Deserialize, Serialize};
    use std::sync::Arc;
    use uuid::Uuid;

    // Helper to create structured output response for negotiation
    fn negotiation_response(skill_id: &str) -> radkit::errors::AgentResult<LlmResponse> {
        let decision = serde_json::json!({
            "type": "start_task",
            "skill_id": skill_id,
            "reasoning": "Test selected this skill"
        });
        Ok(LlmResponse::new(
            Content::from_text(serde_json::to_string(&decision).expect("valid JSON")),
            TokenUsage::empty(),
        ))
    }

    // Helper to create a simple text message
    fn create_message(text: &str, context_id: Option<String>, task_id: Option<String>) -> Message {
        Message {
            kind: "message".to_string(),
            message_id: Uuid::new_v4().to_string(),
            role: MessageRole::User,
            parts: vec![Part::Text {
                text: text.to_string(),
                metadata: None,
            }],
            context_id,
            task_id,
            reference_task_ids: vec![],
            extensions: vec![],
            metadata: None,
        }
    }

    // ============================================================================
    // Test 1: New task creation and immediate completion
    // ============================================================================

    struct ImmediateSkill;

    static IMMEDIATE_METADATA: SkillMetadata = SkillMetadata::new(
        "immediate",
        "Immediate Skill",
        "Completes immediately",
        &[],
        &[],
        &[],
        &[],
    );

    #[cfg_attr(
        all(target_os = "wasi", target_env = "p1"),
        async_trait::async_trait(?Send)
    )]
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
            _content: Content,
        ) -> Result<OnRequestResult, AgentError> {
            Ok(OnRequestResult::Completed {
                message: Some(Content::from_text("Task completed immediately!")),
                artifacts: vec![],
            })
        }

        async fn on_input_received(
            &self,
            _task_context: &mut TaskContext,
            _context: &Context,
            _runtime: &dyn AgentRuntime,
            _input: Content,
        ) -> Result<OnInputResult, AgentError> {
            unreachable!("immediate skill should not receive input")
        }
    }

    impl RegisteredSkill for ImmediateSkill {
        fn metadata() -> &'static SkillMetadata {
            &IMMEDIATE_METADATA
        }
    }

    #[tokio::test]
    async fn test_new_task_immediate_completion() {
        // Provide structured output response for skill negotiation
        let llm = FakeLlm::with_responses("fake-llm", [negotiation_response("immediate")]);

        let runtime = Runtime::builder(Agent::builder().with_skill(ImmediateSkill).build(), llm)
            .build()
            .into_shared();
        let executor_runtime: Arc<dyn ExecutorRuntime> = runtime.clone();
        let executor = RequestExecutor::new(executor_runtime);

        let params = MessageSendParams {
            message: create_message("Hello", None, None),
            configuration: None,
            metadata: None,
        };

        let result = executor.handle_send_message(params).await;
        if let Err(ref e) = result {
            eprintln!("Error: {:?}", e);
        }
        assert!(result.is_ok(), "send_message should succeed");

        // Should return a task with Completed status
        let send_result = result.unwrap();
        match send_result {
            a2a_types::SendMessageResult::Task(task) => {
                assert_eq!(task.status.state, a2a_types::TaskState::Completed);
                assert!(!task.history.is_empty(), "should have messages in history");
            }
            _ => panic!("expected Task result"),
        }
    }

    // ============================================================================
    // Test 2: Task requires input and continuation
    // ============================================================================

    #[derive(Serialize, Deserialize, Clone, Debug)]
    enum GreetingSlot {
        AwaitingName,
    }

    struct GreetingSkill;

    static GREETING_METADATA: SkillMetadata = SkillMetadata::new(
        "greeting",
        "Greeting Skill",
        "Greets user by name",
        &[],
        &[],
        &[],
        &[],
    );

    #[cfg_attr(
        all(target_os = "wasi", target_env = "p1"),
        async_trait::async_trait(?Send)
    )]
    #[cfg_attr(
        not(all(target_os = "wasi", target_env = "p1")),
        async_trait::async_trait
    )]
    impl SkillHandler for GreetingSkill {
        async fn on_request(
            &self,
            _task_context: &mut TaskContext,
            _context: &Context,
            _runtime: &dyn AgentRuntime,
            _content: Content,
        ) -> Result<OnRequestResult, AgentError> {
            Ok(OnRequestResult::InputRequired {
                message: Content::from_text("What is your name?"),
                slot: SkillSlot::new(GreetingSlot::AwaitingName),
            })
        }

        async fn on_input_received(
            &self,
            task_context: &mut TaskContext,
            _context: &Context,
            _runtime: &dyn AgentRuntime,
            input: Content,
        ) -> Result<OnInputResult, AgentError> {
            let slot: GreetingSlot = task_context.load_slot()?.expect("slot should be available");

            match slot {
                GreetingSlot::AwaitingName => {
                    let name = input.first_text().unwrap_or("Friend");
                    Ok(OnInputResult::Completed {
                        message: Some(Content::from_text(format!("Hello, {}!", name))),
                        artifacts: vec![],
                    })
                }
            }
        }
    }

    impl RegisteredSkill for GreetingSkill {
        fn metadata() -> &'static SkillMetadata {
            &GREETING_METADATA
        }
    }

    #[tokio::test]
    async fn test_task_continuation_with_input() {
        let llm = FakeLlm::with_responses("fake-llm", [negotiation_response("greeting")]);

        let runtime = Runtime::builder(Agent::builder().with_skill(GreetingSkill).build(), llm)
            .build()
            .into_shared();
        let executor_runtime: Arc<dyn ExecutorRuntime> = runtime.clone();
        let executor = RequestExecutor::new(executor_runtime);

        // Step 1: Send initial request
        let params1 = MessageSendParams {
            message: create_message("Greet me", None, None),
            configuration: None,
            metadata: None,
        };

        let result1 = executor.handle_send_message(params1).await.unwrap();
        let (context_id, task_id) = match result1 {
            a2a_types::SendMessageResult::Task(task) => {
                assert_eq!(task.status.state, a2a_types::TaskState::InputRequired);
                (task.context_id, task.id)
            }
            _ => panic!("expected Task result"),
        };

        // Step 2: Continue with input
        let params2 = MessageSendParams {
            message: create_message("Alice", Some(context_id), Some(task_id)),
            configuration: None,
            metadata: None,
        };

        let result2 = executor.handle_send_message(params2).await.unwrap();
        match result2 {
            a2a_types::SendMessageResult::Task(task) => {
                assert_eq!(task.status.state, a2a_types::TaskState::Completed);
                // Find the assistant's final message
                let final_msg = task
                    .history
                    .iter()
                    .rfind(|msg| msg.role == MessageRole::Agent)
                    .expect("should have agent message");
                let text = match &final_msg.parts[0] {
                    Part::Text { text, .. } => text,
                    _ => panic!("expected text content"),
                };
                assert!(text.contains("Hello, Alice!"), "should greet by name");
            }
            _ => panic!("expected Task result"),
        }
    }

    // ============================================================================
    // Test 3: Task failure
    // ============================================================================

    struct FailingSkill;

    static FAILING_METADATA: SkillMetadata = SkillMetadata::new(
        "failing",
        "Failing Skill",
        "Always fails",
        &[],
        &[],
        &[],
        &[],
    );

    #[cfg_attr(
        all(target_os = "wasi", target_env = "p1"),
        async_trait::async_trait(?Send)
    )]
    #[cfg_attr(
        not(all(target_os = "wasi", target_env = "p1")),
        async_trait::async_trait
    )]
    impl SkillHandler for FailingSkill {
        async fn on_request(
            &self,
            _task_context: &mut TaskContext,
            _context: &Context,
            _runtime: &dyn AgentRuntime,
            _content: Content,
        ) -> Result<OnRequestResult, AgentError> {
            Err(AgentError::Internal {
                component: "FailingSkill".to_string(),
                reason: "Intentional failure for testing".to_string(),
            })
        }

        async fn on_input_received(
            &self,
            _task_context: &mut TaskContext,
            _context: &Context,
            _runtime: &dyn AgentRuntime,
            _input: Content,
        ) -> Result<OnInputResult, AgentError> {
            unreachable!()
        }
    }

    impl RegisteredSkill for FailingSkill {
        fn metadata() -> &'static SkillMetadata {
            &FAILING_METADATA
        }
    }

    #[tokio::test]
    async fn test_task_failure() {
        let llm = FakeLlm::with_responses("fake-llm", [negotiation_response("failing")]);

        let runtime = Runtime::builder(Agent::builder().with_skill(FailingSkill).build(), llm)
            .build()
            .into_shared();
        let executor_runtime: Arc<dyn ExecutorRuntime> = runtime.clone();
        let executor = RequestExecutor::new(executor_runtime);

        let params = MessageSendParams {
            message: create_message("Do something", None, None),
            configuration: None,
            metadata: None,
        };

        let result = executor.handle_send_message(params).await;

        // The executor returns the skill error directly when skill fails during on_request
        assert!(result.is_err(), "Should fail when skill throws error");
        match result.unwrap_err() {
            AgentError::Internal { component, reason } => {
                assert_eq!(component, "FailingSkill");
                assert!(reason.contains("Intentional failure"));
            }
            other => panic!("Expected Internal error, got {:?}", other),
        }
    }

    // ============================================================================
    // Test 4: Task retrieval
    // ============================================================================

    #[tokio::test]
    async fn test_get_task() {
        let llm = FakeLlm::with_responses("fake-llm", [negotiation_response("immediate")]);

        let runtime = Runtime::builder(Agent::builder().with_skill(ImmediateSkill).build(), llm)
            .build()
            .into_shared();
        let executor_runtime: Arc<dyn ExecutorRuntime> = runtime.clone();
        let executor = RequestExecutor::new(executor_runtime);

        // Create a task
        let params = MessageSendParams {
            message: create_message("test", None, None),
            configuration: None,
            metadata: None,
        };

        let send_result = executor.handle_send_message(params).await.unwrap();
        let task_id = match send_result {
            a2a_types::SendMessageResult::Task(task) => task.id,
            _ => panic!("expected Task"),
        };

        // Retrieve the task
        let get_result = executor
            .handle_get_task(TaskQueryParams {
                id: task_id.clone(),
                history_length: None,
                metadata: None,
            })
            .await;

        assert!(get_result.is_ok(), "should retrieve task");
        let retrieved_task = get_result.unwrap();
        assert_eq!(retrieved_task.id, task_id);
        assert_eq!(retrieved_task.status.state, a2a_types::TaskState::Completed);
    }

    // ============================================================================
    // Test 5: Invalid task ID
    // ============================================================================

    #[tokio::test]
    async fn test_get_nonexistent_task() {
        let llm = FakeLlm::with_responses("fake-llm", [negotiation_response("immediate")]);

        let runtime = Runtime::builder(Agent::builder().with_skill(ImmediateSkill).build(), llm)
            .build()
            .into_shared();
        let executor_runtime: Arc<dyn ExecutorRuntime> = runtime.clone();
        let executor = RequestExecutor::new(executor_runtime);

        let result = executor
            .handle_get_task(TaskQueryParams {
                id: "nonexistent-task-id".to_string(),
                history_length: None,
                metadata: None,
            })
            .await;

        assert!(result.is_err(), "should fail for nonexistent task");
        match result.unwrap_err() {
            AgentError::TaskNotFound { task_id } => {
                assert_eq!(task_id, "nonexistent-task-id");
            }
            _ => panic!("expected TaskNotFound error"),
        }
    }

    // ============================================================================
    // Test 6: Continue with wrong context_id/task_id combination
    // ============================================================================

    #[tokio::test]
    async fn test_invalid_context_task_combination() {
        let llm = FakeLlm::with_responses("fake-llm", [negotiation_response("greeting")]);

        let runtime = Runtime::builder(Agent::builder().with_skill(GreetingSkill).build(), llm)
            .build()
            .into_shared();
        let executor_runtime: Arc<dyn ExecutorRuntime> = runtime.clone();
        let executor = RequestExecutor::new(executor_runtime);

        // Create first task
        let params1 = MessageSendParams {
            message: create_message("Hello", None, None),
            configuration: None,
            metadata: None,
        };

        let result1 = executor.handle_send_message(params1).await.unwrap();
        let task1_id = match result1 {
            a2a_types::SendMessageResult::Task(task) => task.id,
            _ => panic!("expected Task"),
        };

        // Try to use that task_id with a different context_id
        let params2 = MessageSendParams {
            message: create_message(
                "Continue",
                Some("wrong-context-id".to_string()),
                Some(task1_id),
            ),
            configuration: None,
            metadata: None,
        };

        let result2 = executor.handle_send_message(params2).await;
        assert!(result2.is_err(), "should fail with mismatched context/task");
    }
}
