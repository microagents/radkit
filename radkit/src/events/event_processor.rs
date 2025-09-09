use crate::errors::AgentResult;
use crate::events::EventBus;
use crate::sessions::{SessionEvent, SessionEventType, SessionService};
use std::sync::Arc;

/// EventProcessor handles the business logic of event processing
/// while keeping SessionService as a pure persistence layer.
///
/// This provides clean separation of concerns:
/// - SessionService: Pure persistence (store/retrieve events)
/// - EventProcessor: Business logic (indexing, side effects, streaming)
pub struct EventProcessor {
    /// Pure persistence layer - can be InMemory, Database, etc.
    session_service: Arc<dyn SessionService>,

    /// Event streaming for real-time subscriptions
    event_bus: Arc<dyn EventBus>,
}

impl EventProcessor {
    pub fn new(session_service: Arc<dyn SessionService>, event_bus: Arc<dyn EventBus>) -> Self {
        Self {
            session_service,
            event_bus,
        }
    }

    /// Get access to the event bus for subscriptions
    pub fn event_bus(&self) -> &Arc<dyn EventBus> {
        &self.event_bus
    }

    /// Process an event with full business logic
    /// This replaces the complex logic that was in InMemorySessionService
    pub async fn process_event(&self, event: SessionEvent) -> AgentResult<()> {
        // 1. Store event in persistence layer (includes indexing)
        self.session_service.store_event(&event).await?;

        // 2. Apply side effects (state changes)
        self.apply_event_side_effects(&event).await?;

        // 3. Stream to subscribers via EventBus
        self.event_bus.publish(event).await?;

        Ok(())
    }

    /// Apply side effects like updating app/user/session state
    async fn apply_event_side_effects(&self, event: &SessionEvent) -> AgentResult<()> {
        match &event.event_type {
            SessionEventType::StateChanged { scope, .. } => {
                // Delegate state changes to SessionService
                match scope {
                    crate::sessions::StateScope::App => {
                        // Extract app_name from session (this requires a session lookup)
                        // For now, we'll let the SessionService handle this internally
                        self.session_service.apply_state_change(event).await?;
                    }
                    crate::sessions::StateScope::User => {
                        self.session_service.apply_state_change(event).await?;
                    }
                    crate::sessions::StateScope::Session => {
                        self.session_service.apply_state_change(event).await?;
                    }
                }
            }
            _ => {
                // Other events don't have side effects
            }
        }
        Ok(())
    }
}
