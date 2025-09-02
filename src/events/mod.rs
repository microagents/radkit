// Unified event system
pub mod event_bus;
pub mod event_processor;
pub mod execution_context;

// Export the unified event system
pub use event_bus::{EventBus, InMemoryEventBus, Subscription};
pub use event_processor::EventProcessor;
pub use execution_context::ExecutionContext;
