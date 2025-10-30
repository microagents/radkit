//! Core runtime components (advanced API).
//!
//! This module contains the core execution machinery and framework internals.
//! Most users won't need to interact with these components directly - they're
//! used internally by the runtime to orchestrate agent execution.
//!
//! ⚠️ **Advanced API**: These components are considered internal framework
//! machinery. The API surface may change more frequently than user-facing
//! services. Advanced users can customize these components if needed.
//!
//! # Modules
//!
//! - [`executor`] - Task execution logic (native + runtime feature only)
//! - [`wasm`] - WASM runtime utilities
//! - [`negotiator`] - Message negotiation and routing
//! - [`status_mapper`] - A2A protocol status conversion utilities

pub mod event_bus;
#[cfg(all(feature = "runtime", not(all(target_os = "wasi", target_env = "p1"))))]
pub mod executor;
pub mod negotiator;
pub mod status_mapper;
pub mod wasm;

// Re-export commonly used types
pub use event_bus::{TaskEventBus, TaskEventReceiver};
pub use negotiator::{DefaultNegotiator, NegotiationDecision, Negotiator};
