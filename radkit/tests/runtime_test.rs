#![cfg(feature = "runtime")]

use a2a_types::{Message, MessageRole, Part, TaskState, TaskStatus};
use radkit::runtime::context::AuthContext;
use radkit::runtime::task_manager::{
    InMemoryTaskManager, ListTasksFilter, Task, TaskEvent, TaskManager,
};

fn create_auth_context(app: &str, user: &str) -> AuthContext {
    AuthContext {
        app_name: app.to_string(),
        user_name: user.to_string(),
    }
}

fn create_task(id: &str, session_id: &str) -> Task {
    Task {
        id: id.to_string(),
        context_id: session_id.to_string(),
        status: TaskStatus {
            state: TaskState::Submitted,
            timestamp: None,
            message: None,
        },
        artifacts: vec![],
    }
}

#[tokio::test]
async fn test_task_manager_save_and_get_task() {
    let manager = InMemoryTaskManager::new();
    let auth_ctx = create_auth_context("app1", "user1");
    let task = create_task("task1", "session1");

    manager.save_task(&auth_ctx, &task).await.unwrap();

    let fetched = manager.get_task(&auth_ctx, "task1").await.unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().id, "task1");
}

#[tokio::test]
async fn test_task_manager_get_task_not_found() {
    let manager = InMemoryTaskManager::new();
    let auth_ctx = create_auth_context("app1", "user1");

    let fetched = manager.get_task(&auth_ctx, "nonexistent").await.unwrap();
    assert!(fetched.is_none());
}

#[tokio::test]
async fn test_task_manager_auth_scoping_get() {
    let manager = InMemoryTaskManager::new();
    let auth_ctx1 = create_auth_context("app1", "user1");
    let auth_ctx2 = create_auth_context("app1", "user2");
    let task = create_task("task1", "session1");

    manager.save_task(&auth_ctx1, &task).await.unwrap();

    // Correct user can fetch
    let fetched1 = manager.get_task(&auth_ctx1, "task1").await.unwrap();
    assert!(fetched1.is_some());

    // Different user cannot
    let fetched2 = manager
        .get_task(
            &auth_ctx2, "task1
",
        )
        .await
        .unwrap();
    assert!(fetched2.is_none());
}

#[tokio::test]
async fn test_task_manager_add_and_get_events() {
    let manager = InMemoryTaskManager::new();
    let auth_ctx = create_auth_context("app1", "user1");
    let task_id = "task1";

    let msg = Message {
        kind: "message".to_string(),
        message_id: "msg1".to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: "Hello".to_string(),
            metadata: None,
        }],
        context_id: None,
        taskId: None,
        reference_task_ids: vec![],
        extensions: vec![],
        metadata: None,
    };
    let event = TaskEvent::Message(msg);

    manager
        .add_task_event(&auth_ctx, task_id, &event)
        .await
        .unwrap();
    manager
        .add_task_event(&auth_ctx, task_id, &event)
        .await
        .unwrap();

    let events = manager.get_task_events(&auth_ctx, task_id).await.unwrap();
    assert_eq!(events.len(), 2);
}

#[tokio::test]
async fn test_task_manager_list_tasks() {
    let manager = InMemoryTaskManager::new();
    let auth_ctx = create_auth_context("app1", "user1");

    manager
        .save_task(&auth_ctx, &create_task("task1", "session1"))
        .await
        .unwrap();
    manager
        .save_task(&auth_ctx, &create_task("task2", "session1"))
        .await
        .unwrap();
    manager
        .save_task(&auth_ctx, &create_task("task3", "session2"))
        .await
        .unwrap();

    // List all tasks for user
    let all_tasks = manager
        .list_tasks(&auth_ctx, &ListTasksFilter::default())
        .await
        .unwrap();
    assert_eq!(all_tasks.items.len(), 3);

    // Filter by session
    let session1_tasks = manager
        .list_tasks(
            &auth_ctx,
            &ListTasksFilter {
                context_id: Some("session1"),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(session1_tasks.items.len(), 2);
}
