//! Integration tests for runtime services.
//!
//! These tests verify that the runtime services (TaskManager, MemoryService, LoggingService,
//! AuthService) work correctly together within a runtime handle.

use radkit::agent::Agent;
use radkit::runtime::context::AuthContext;
use radkit::runtime::task_manager::InMemoryTaskManager;
use radkit::runtime::{AgentRuntime, ListTasksFilter, LogLevel, MemoryServiceExt, Runtime};
use radkit::test_support::FakeLlm;

fn test_agent() -> radkit::agent::AgentDefinition {
    Agent::builder()
        .with_id("test-agent")
        .with_name("Test Agent")
        .build()
}

fn runtime_with_manager(llm: FakeLlm) -> Runtime {
    Runtime::builder(test_agent(), llm)
        .with_task_manager(InMemoryTaskManager::new())
        .build()
}

#[tokio::test]
async fn test_auth_service_provides_context() {
    let llm = FakeLlm::with_responses("fake_llm", std::iter::empty());
    let runtime = Runtime::builder(test_agent(), llm).build();

    let auth_context = runtime.auth().get_auth_context();

    assert_eq!(auth_context.app_name, "default-app");
    assert_eq!(auth_context.user_name, "default-user");
}

#[tokio::test]
async fn test_task_manager_save_and_get() {
    let llm = FakeLlm::with_responses("fake_llm", std::iter::empty());
    let runtime = runtime_with_manager(llm);
    let task_manager = runtime.task_manager();

    let auth_context = runtime.auth().get_auth_context();

    use a2a_types::{TaskState, TaskStatus};
    use radkit::runtime::task_manager::Task;

    // Create a task
    let task = Task {
        id: "test-task-1".to_string(),
        context_id: "test-context-1".to_string(),
        status: TaskStatus {
            state: TaskState::Working,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            message: None,
        },
        artifacts: vec![],
    };

    task_manager
        .save_task(&auth_context, &task)
        .await
        .expect("save task");

    // Retrieve the task
    let retrieved = task_manager
        .get_task(&auth_context, "test-task-1")
        .await
        .expect("get task")
        .expect("task should exist");

    assert_eq!(retrieved.id, "test-task-1");
    assert_eq!(retrieved.context_id, "test-context-1");
}

#[tokio::test]
async fn test_task_manager_auth_scoping() {
    let llm = FakeLlm::with_responses("fake_llm", std::iter::empty());
    let runtime = runtime_with_manager(llm).into_shared();
    let task_manager = runtime.task_manager();

    // Create two different auth contexts
    let auth_context_1 = AuthContext {
        app_name: "app1".to_string(),
        user_name: "user1".to_string(),
    };

    let auth_context_2 = AuthContext {
        app_name: "app2".to_string(),
        user_name: "user2".to_string(),
    };

    use a2a_types::{TaskState, TaskStatus};
    use radkit::runtime::task_manager::Task;

    // Save task for auth_context_1
    let task1 = Task {
        id: "task-1".to_string(),
        context_id: "context-1".to_string(),
        status: TaskStatus {
            state: TaskState::Working,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            message: None,
        },
        artifacts: vec![],
    };

    task_manager
        .save_task(&auth_context_1, &task1)
        .await
        .expect("save task for auth1");

    // Save task for auth_context_2
    let task2 = Task {
        id: "task-2".to_string(),
        context_id: "context-2".to_string(),
        status: TaskStatus {
            state: TaskState::Working,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            message: None,
        },
        artifacts: vec![],
    };

    task_manager
        .save_task(&auth_context_2, &task2)
        .await
        .expect("save task for auth2");

    // Verify auth_context_1 can only see its own task
    let retrieved = task_manager
        .get_task(&auth_context_1, "task-1")
        .await
        .expect("get task")
        .expect("task should exist");
    assert_eq!(retrieved.id, "task-1");

    let not_found = task_manager
        .get_task(&auth_context_1, "task-2")
        .await
        .expect("get task");
    assert!(not_found.is_none(), "auth_context_1 should not see task-2");

    // Verify auth_context_2 can only see its own task
    let retrieved = task_manager
        .get_task(&auth_context_2, "task-2")
        .await
        .expect("get task")
        .expect("task should exist");
    assert_eq!(retrieved.id, "task-2");

    let not_found = task_manager
        .get_task(&auth_context_2, "task-1")
        .await
        .expect("get task");
    assert!(not_found.is_none(), "auth_context_2 should not see task-1");
}

#[tokio::test]
async fn test_task_manager_list_tasks() {
    let llm = FakeLlm::with_responses("fake_llm", std::iter::empty());
    let runtime = runtime_with_manager(llm).into_shared();
    let task_manager = runtime.task_manager();
    let auth_context = runtime.auth().get_auth_context();

    use a2a_types::{TaskState, TaskStatus};
    use radkit::runtime::task_manager::Task;

    // Create multiple tasks
    for i in 1..=5 {
        let task = Task {
            id: format!("task-{i}"),
            context_id: format!("context-{i}"),
            status: TaskStatus {
                state: TaskState::Working,
                timestamp: Some(chrono::Utc::now().to_rfc3339()),
                message: None,
            },
            artifacts: vec![],
        };

        task_manager
            .save_task(&auth_context, &task)
            .await
            .expect("save task");
    }

    // List all tasks
    let filter = ListTasksFilter::default();
    let result = task_manager
        .list_tasks(&auth_context, &filter)
        .await
        .expect("list tasks");

    assert!(result.items.len() >= 5, "Should have at least 5 tasks");
}

#[tokio::test]
async fn test_memory_service_save_and_load() {
    let llm = FakeLlm::with_responses("fake_llm", std::iter::empty());
    let runtime = Runtime::builder(test_agent(), llm).build();

    let auth = runtime.auth();
    let auth_context = auth.get_auth_context();
    let memory = runtime.memory();

    // Save a value
    let key = "test_key";
    let value = serde_json::json!({"name": "Alice", "age": 30});

    memory
        .save(&auth_context, key, &value)
        .await
        .expect("save value");

    // Retrieve the value
    let retrieved: serde_json::Value = memory
        .load(&auth_context, key)
        .await
        .expect("load value")
        .expect("value should exist");

    assert_eq!(retrieved, value);
}

#[tokio::test]
async fn test_memory_service_auth_scoping() {
    let llm = FakeLlm::with_responses("fake_llm", std::iter::empty());
    let runtime = Runtime::builder(test_agent(), llm).build();

    let memory = runtime.memory();

    // Create two different auth contexts
    let auth_context_1 = AuthContext {
        app_name: "app1".to_string(),
        user_name: "user1".to_string(),
    };

    let auth_context_2 = AuthContext {
        app_name: "app2".to_string(),
        user_name: "user2".to_string(),
    };

    // Save value for auth_context_1
    let value1 = serde_json::json!({"data": "context1"});
    memory
        .save(&auth_context_1, "shared_key", &value1)
        .await
        .expect("save value for auth1");

    // Save value for auth_context_2
    let value2 = serde_json::json!({"data": "context2"});
    memory
        .save(&auth_context_2, "shared_key", &value2)
        .await
        .expect("save value for auth2");

    // Verify auth_context_1 sees its own value
    let retrieved1: serde_json::Value = memory
        .load(&auth_context_1, "shared_key")
        .await
        .expect("load value")
        .expect("value should exist");
    assert_eq!(retrieved1["data"], "context1");

    // Verify auth_context_2 sees its own value
    let retrieved2: serde_json::Value = memory
        .load(&auth_context_2, "shared_key")
        .await
        .expect("load value")
        .expect("value should exist");
    assert_eq!(retrieved2["data"], "context2");
}

#[tokio::test]
async fn test_logging_service() {
    let llm = FakeLlm::with_responses("fake_llm", std::iter::empty());
    let runtime = Runtime::builder(test_agent(), llm).build();

    let logging = runtime.logging();

    // Test logging at different levels (should not panic)
    logging.log(LogLevel::Info, "Test info message");
    logging.log(LogLevel::Warn, "Test warn message");
    logging.log(LogLevel::Error, "Test error message");
    logging.log(LogLevel::Debug, "Test debug message");
}

#[tokio::test]
async fn test_runtime_services_together() {
    // Test that all services can be used together in a workflow
    let llm = FakeLlm::with_responses("fake_llm", std::iter::empty());
    let runtime = runtime_with_manager(llm).into_shared();
    let task_manager = runtime.task_manager();

    let auth = runtime.auth();
    let auth_context = auth.get_auth_context();

    // Use logging
    runtime.logging().log(LogLevel::Info, "Starting workflow");

    // Use memory to store workflow state
    let workflow_state = serde_json::json!({"step": 1, "data": "processing"});
    runtime
        .memory()
        .save(&auth_context, "workflow_state", &workflow_state)
        .await
        .expect("save state");

    // Create a task
    use a2a_types::{TaskState, TaskStatus};
    use radkit::runtime::task_manager::Task;

    let task = Task {
        id: "workflow-task-1".to_string(),
        context_id: "workflow-context-1".to_string(),
        status: TaskStatus {
            state: TaskState::Working,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            message: None,
        },
        artifacts: vec![],
    };

    task_manager
        .save_task(&auth_context, &task)
        .await
        .expect("save task");

    // Retrieve workflow state
    let retrieved_state: serde_json::Value = runtime
        .memory()
        .load(&auth_context, "workflow_state")
        .await
        .expect("load state")
        .expect("state should exist");

    assert_eq!(retrieved_state["step"], 1);

    // Retrieve task
    let retrieved_task = task_manager
        .get_task(&auth_context, "workflow-task-1")
        .await
        .expect("get task")
        .expect("task should exist");

    assert_eq!(retrieved_task.id, "workflow-task-1");

    // Log completion
    runtime.logging().log(LogLevel::Info, "Workflow completed");
}
