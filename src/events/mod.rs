// Event projection system
pub mod execution_context;
pub mod internal;
pub mod projection;

// Export the event projection system
pub use execution_context::ProjectedExecutionContext;
pub use internal::InternalEvent;
pub use projection::{
    A2ACaptureProjector, A2AOnlyProjector, EventCaptureProjector, EventGenerators, EventProjector,
    InternalOnlyProjector, MultiProjector, StorageProjector,
};
