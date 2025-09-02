use crate::errors::AgentResult;
use crate::sessions::session_event::{EventFilter, SessionEvent};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

/// EventBus trait for streaming events to subscribers
/// Provides publish/subscribe pattern for real-time event distribution
#[async_trait]
pub trait EventBus: Send + Sync {
    /// Publish an event to all subscribers
    async fn publish(&self, event: SessionEvent) -> AgentResult<()>;

    /// Subscribe to events with a filter
    /// Returns a receiver stream for filtered events
    async fn subscribe(&self, filter: EventFilter) -> AgentResult<mpsc::Receiver<SessionEvent>>;

    /// Get subscriber count (useful for debugging/monitoring)
    async fn subscriber_count(&self) -> usize;
}

/// Subscription handle for managing event streams
#[derive(Debug)]
pub struct Subscription {
    pub id: String,
    pub filter: EventFilter,
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

        // Send to all matching subscribers
        for subscription in subscribers.iter() {
            if event.matches_filter(&subscription.filter) {
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

    async fn subscribe(&self, filter: EventFilter) -> AgentResult<mpsc::Receiver<SessionEvent>> {
        // Clean up closed subscribers
        self.cleanup_closed_subscribers().await;

        let (sender, receiver) = mpsc::channel(1000); // Buffer up to 1000 events

        let subscription = Subscription {
            id: uuid::Uuid::new_v4().to_string(),
            filter,
            sender,
        };

        let mut subscribers = self.subscribers.write().await;
        subscribers.push(subscription);

        Ok(receiver)
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
    use crate::a2a::MessageRole;
    use crate::models::content::Content;
    use crate::sessions::session_event::{SessionEvent, SessionEventType};

    #[tokio::test]
    async fn test_event_bus_publish_subscribe() {
        let event_bus = InMemoryEventBus::new();

        // Subscribe to all events
        let mut receiver = event_bus.subscribe(EventFilter::All).await.unwrap();

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
    async fn test_event_bus_filtering() {
        let event_bus = InMemoryEventBus::new();

        // Subscribe to conversation events only
        let mut conversation_receiver = event_bus
            .subscribe(EventFilter::ConversationOnly)
            .await
            .unwrap();

        // Subscribe to specific task events
        let mut task_receiver = event_bus
            .subscribe(EventFilter::TaskOnly("task1".to_string()))
            .await
            .unwrap();

        // Create a user message event (should match conversation filter)
        let content = Content::new(
            "task1".to_string(),
            "session1".to_string(),
            "msg1".to_string(),
            MessageRole::User,
        );

        let user_event = SessionEvent::new(
            "session1".to_string(),
            "task1".to_string(),
            SessionEventType::UserMessage { content },
        );

        // Create a state change event (should not match conversation filter)
        let state_event = SessionEvent::new(
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
        event_bus.publish(user_event.clone()).await.unwrap();
        event_bus.publish(state_event.clone()).await.unwrap();

        // Conversation receiver should get user message only
        let received_conversation = conversation_receiver.recv().await.unwrap();
        assert_eq!(received_conversation.task_id, "task1");

        // Task receiver should get user message only (task1 filter)
        let received_task = task_receiver.recv().await.unwrap();
        assert_eq!(received_task.task_id, "task1");

        // No more events should be received by conversation receiver
        assert!(conversation_receiver.try_recv().is_err());

        // No more events should be received by task receiver
        assert!(task_receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_subscriber_count() {
        let event_bus = InMemoryEventBus::new();

        assert_eq!(event_bus.subscriber_count().await, 0);

        let _receiver1 = event_bus.subscribe(EventFilter::All).await.unwrap();
        assert_eq!(event_bus.subscriber_count().await, 1);

        let _receiver2 = event_bus.subscribe(EventFilter::A2A).await.unwrap();
        assert_eq!(event_bus.subscriber_count().await, 2);

        // Drop one receiver
        drop(_receiver1);

        // Count should be updated after cleanup (triggered by next operation)
        let _receiver3 = event_bus
            .subscribe(EventFilter::ConversationOnly)
            .await
            .unwrap();
        assert_eq!(event_bus.subscriber_count().await, 2); // _receiver2 + _receiver3
    }
}
