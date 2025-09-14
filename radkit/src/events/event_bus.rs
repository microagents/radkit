use crate::errors::AgentResult;
use crate::sessions::session_event::SessionEvent;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

/// EventBus trait for streaming events to subscribers
/// Provides publish/subscribe pattern for real-time event distribution
#[async_trait]
pub trait EventBus: Send + Sync {
    /// Publish an event to all subscribers
    async fn publish(&self, event: SessionEvent) -> AgentResult<()>;

    /// Subscribe to events for a specific task
    /// Returns a receiver stream for events from that task only
    async fn subscribe(&self, task_id: String) -> AgentResult<mpsc::Receiver<SessionEvent>>;

    /// Unsubscribe all subscriptions for a specific task
    /// This closes all associated channels and removes subscriptions
    async fn unsubscribe(&self, task_id: String) -> AgentResult<()>;

    /// Get subscriber count (useful for debugging/monitoring)
    async fn subscriber_count(&self) -> usize;
}

/// Subscription handle for managing event streams
#[derive(Debug)]
pub struct Subscription {
    pub id: String,
    pub task_id: String,
    pub sender: mpsc::Sender<SessionEvent>,
}

/// In-memory EventBus implementation using tokio channels
/// Suitable for single-process deployments
pub struct InMemoryEventBus {
    subscribers: Arc<tokio::sync::RwLock<Vec<Subscription>>>,
}

impl InMemoryEventBus {
    /// Create a new in-memory event bus
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        }
    }

    /// Clean up closed subscribers
    async fn cleanup_closed_subscribers(&self) {
        let mut subscribers = self.subscribers.write().await;
        subscribers.retain(|sub| !sub.sender.is_closed());
    }
}

impl Default for InMemoryEventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventBus for InMemoryEventBus {
    async fn publish(&self, event: SessionEvent) -> AgentResult<()> {
        // Clean up closed subscribers first
        self.cleanup_closed_subscribers().await;

        let subscribers = self.subscribers.read().await;

        // Send to all subscribers for this task
        for subscription in subscribers.iter() {
            if subscription.task_id == event.task_id {
                // Use try_send to avoid blocking if subscriber is slow
                if subscription.sender.try_send(event.clone()).is_err() {
                    // Subscriber channel is full or closed, but we don't fail the publish
                    // The cleanup will remove closed channels on next publish/subscribe
                    continue;
                }
            }
        }

        Ok(())
    }

    async fn subscribe(&self, task_id: String) -> AgentResult<mpsc::Receiver<SessionEvent>> {
        // Clean up closed subscribers
        self.cleanup_closed_subscribers().await;

        let (sender, receiver) = mpsc::channel(1000); // Buffer up to 1000 events

        let subscription = Subscription {
            id: uuid::Uuid::new_v4().to_string(),
            task_id,
            sender,
        };

        let mut subscribers = self.subscribers.write().await;
        subscribers.push(subscription);

        Ok(receiver)
    }

    async fn unsubscribe(&self, task_id: String) -> AgentResult<()> {
        let mut subscribers = self.subscribers.write().await;

        // Find and remove all subscriptions for this task
        let initial_count = subscribers.len();
        subscribers.retain(|sub| sub.task_id != task_id);
        let removed_count = initial_count - subscribers.len();

        if removed_count > 0 {
            tracing::debug!(
                "Removed {} subscriptions for task {}",
                removed_count,
                task_id
            );
        }

        Ok(())
    }

    async fn subscriber_count(&self) -> usize {
        self.cleanup_closed_subscribers().await;
        let subscribers = self.subscribers.read().await;
        subscribers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::content::Content;
    use crate::sessions::session_event::{SessionEvent, SessionEventType};
    use a2a_types::MessageRole;

    #[tokio::test]
    async fn test_event_bus_publish_subscribe() {
        let event_bus = InMemoryEventBus::new();

        // Subscribe to events for task1
        let mut receiver = event_bus.subscribe("task1".to_string()).await.unwrap();

        // Create a test event
        let content = Content::new(
            "task1".to_string(),
            "session1".to_string(),
            "msg1".to_string(),
            MessageRole::User,
        );

        let event = SessionEvent::new(
            "session1".to_string(),
            "task1".to_string(),
            SessionEventType::UserMessage { content },
        );

        // Publish the event
        event_bus.publish(event.clone()).await.unwrap();

        // Receive the event
        let received_event = receiver.recv().await.unwrap();
        assert_eq!(received_event.session_id, event.session_id);
        assert_eq!(received_event.task_id, event.task_id);
    }

    #[tokio::test]
    async fn test_task_based_filtering() {
        let event_bus = InMemoryEventBus::new();

        // Subscribe to task1 events only
        let mut task1_receiver = event_bus.subscribe("task1".to_string()).await.unwrap();

        // Subscribe to task2 events only
        let mut task2_receiver = event_bus.subscribe("task2".to_string()).await.unwrap();

        // Create events for different tasks
        let task1_event = SessionEvent::new(
            "session1".to_string(),
            "task1".to_string(),
            SessionEventType::UserMessage {
                content: Content::new(
                    "task1".to_string(),
                    "session1".to_string(),
                    "msg1".to_string(),
                    MessageRole::User,
                ),
            },
        );

        let task2_event = SessionEvent::new(
            "session1".to_string(),
            "task2".to_string(),
            SessionEventType::StateChanged {
                scope: crate::sessions::session_event::StateScope::Session,
                key: "theme".to_string(),
                old_value: None,
                new_value: serde_json::json!("dark"),
            },
        );

        // Publish both events
        event_bus.publish(task1_event.clone()).await.unwrap();
        event_bus.publish(task2_event.clone()).await.unwrap();

        // Task1 receiver should only get task1 event
        let received_task1 = task1_receiver.recv().await.unwrap();
        assert_eq!(received_task1.task_id, "task1");

        // Task2 receiver should only get task2 event
        let received_task2 = task2_receiver.recv().await.unwrap();
        assert_eq!(received_task2.task_id, "task2");

        // No more events should be received by either receiver
        assert!(task1_receiver.try_recv().is_err());
        assert!(task2_receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_subscriber_count() {
        let event_bus = InMemoryEventBus::new();

        assert_eq!(event_bus.subscriber_count().await, 0);

        let _receiver1 = event_bus.subscribe("task1".to_string()).await.unwrap();
        assert_eq!(event_bus.subscriber_count().await, 1);

        let _receiver2 = event_bus.subscribe("task2".to_string()).await.unwrap();
        assert_eq!(event_bus.subscriber_count().await, 2);

        // Drop one receiver
        drop(_receiver1);

        // Count should be updated after cleanup (triggered by next operation)
        let _receiver3 = event_bus.subscribe("task3".to_string()).await.unwrap();
        assert_eq!(event_bus.subscriber_count().await, 2); // _receiver2 + _receiver3
    }
}
